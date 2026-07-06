//! no-undefined-assignment oxc backend — flag `= undefined` assignments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentOperator, AssignmentTarget, Expression};
use std::sync::Arc;

pub struct Check;

/// True for an assignment target with no applicable `undefined`-free remediation.
///
/// - `<expr>.current = undefined` — the React ref-clearing idiom. A
///   `MutableRefObject<T>` has `current: T | null` (never `T | undefined`), so
///   `delete ref.current` is a TypeScript error and would break the ref
///   contract. Assigning `undefined` is the intended way to clear the ref.
/// - `<ref>.value = undefined` — the Vue 3 ref-clearing idiom. `.value` is a
///   required property of `Ref<T>`, so `delete ref.value` violates the ref
///   contract; assigning `undefined` is the only way to release the held value
///   (e.g. clearing a DOM-element ref in `onBeforeUnmount`). Recognised only
///   when the base resolves to a Vue ref factory or a composable's `Ref<T>`, so
///   a plain `obj.value = undefined` stays flagged.
fn is_member_target_without_remediation(
    target: &AssignmentTarget,
    semantic: &oxc_semantic::Semantic,
    project: &crate::project::ProjectCtx,
    path: &std::path::Path,
) -> bool {
    let AssignmentTarget::StaticMemberExpression(member) = target else {
        return false;
    };
    member.property.name.as_str() == "current"
        || crate::oxc_helpers::is_vue_ref_value_target(member, semantic, project, path)
        || crate::oxc_helpers::is_destructured_call_ref_value_target(member, semantic)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["undefined"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (is_undefined, span_start) = match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else { return };
                let is_undef =
                    matches!(init, Expression::Identifier(id) if id.name.as_str() == "undefined");
                (is_undef, decl.span.start)
            }
            AstKind::AssignmentExpression(assign) => {
                let is_undef = matches!(
                    &assign.right,
                    Expression::Identifier(id) if id.name.as_str() == "undefined"
                );
                // Re-assigning a plain local variable to `undefined` is the only way
                // to reset it: `let x;` applies to the declaration (already emitted)
                // and `delete obj.prop` applies to properties, not locals. Private
                // fields are exempt because `delete this.#x` is a SyntaxError. The
                // `ref.current` (React) and `<ref>.value` (Vue) member idioms are
                // exempt because `delete` would break the required ref property.
                // Member assignments other than these keep flagging (`delete obj.prop`).
                let target_has_no_remediation = matches!(
                    &assign.left,
                    AssignmentTarget::AssignmentTargetIdentifier(_)
                        | AssignmentTarget::PrivateFieldExpression(_)
                ) || is_member_target_without_remediation(
                    &assign.left,
                    semantic,
                    ctx.project,
                    ctx.path,
                );
                if assign.operator == AssignmentOperator::Assign && target_has_no_remediation {
                    return;
                }
                (is_undef, assign.span.start)
            }
            _ => return,
        };

        if !is_undefined {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not assign `undefined` \u{2014} use `let x;` or `delete obj.prop` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_ref_current_undefined() {
        assert!(run_on("jointRef.current = undefined;").is_empty());
        assert!(run_on("someRef.current = undefined;").is_empty());
    }

    #[test]
    fn flags_let_undefined() {
        assert_eq!(run_on("let x = undefined;").len(), 1);
    }

    #[test]
    fn allows_plain_identifier_reassignment() {
        assert!(run_on("instance = undefined;").is_empty());
    }

    #[test]
    fn allows_reset_let_to_undefined() {
        // hono's `lastError = undefined` — resetting a declared `let` to "no value".
        // `let x;` is for the declaration and `delete obj.prop` is for properties;
        // neither remediation applies to a plain-identifier re-assignment.
        assert!(run_on("lastError = undefined;").is_empty());
    }

    #[test]
    fn flags_member_property_not_current() {
        assert_eq!(run_on("obj.value = undefined;").len(), 1);
    }

    #[test]
    fn allows_vue_ref_value_undefined() {
        // Issue #4731: clearing a Vue reactive ref in `onBeforeUnmount`. `.value`
        // is a required property of `Ref<T>`, so `delete ref.value` is invalid;
        // assigning `undefined` is the only way to release the held value.
        let src = "import { ref, onBeforeUnmount } from 'vue';\n\
                   const focusStartRef = ref<HTMLElement | undefined>(undefined);\n\
                   onBeforeUnmount(() => { focusStartRef.value = undefined; });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_destructured_composable_ref_value_undefined() {
        // A `Ref<T>` returned by a composable, cleared via `.value = undefined`.
        let src = "const { error } = useThing();\n\
                   error.value = undefined;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_non_ref_value_undefined() {
        // A plain object's `.value` (not a Vue ref) keeps flagging: `delete
        // obj.value` is a valid remediation when `obj` is not a ref.
        let src = "const obj = { value: 1 };\n\
                   obj.value = undefined;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_private_field_assignment() {
        // `delete this.#field` is a SyntaxError, so no remediation applies.
        assert!(
            run_on("class C { #handle: number | undefined; clear() { this.#handle = undefined; } }")
                .is_empty()
        );
    }

    /// Run the rule on a source file inside a temp project whose root
    /// `package.json` is `package_json`, so project-context levers (e.g.
    /// `uses_unplugin_auto_import`) resolve against a real manifest.
    fn run_in_project(source: &str, package_json: &str) -> Vec<Diagnostic> {
        use std::fs;
        let dir = tempfile::TempDir::new().expect("tempdir");
        let mut sources: Vec<crate::files::SourceFile> = Vec::new();
        for (rel, body) in [("package.json", package_json), ("src/stores/user.ts", source)] {
            let p = dir.path().join(rel);
            fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            fs::write(&p, body).expect("write");
            if let Some(lang) = crate::files::Language::from_path(&p) {
                sources.push(crate::files::SourceFile { path: p, language: lang });
            }
        }
        let refs: Vec<&crate::files::SourceFile> = sources.iter().collect();
        let project = crate::project::ProjectCtx::load(&refs, &crate::config::Config::default());
        let src_path = dir.path().join("src/stores/user.ts");
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &src_path,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn allows_auto_imported_vue_ref_value_undefined() {
        // Issue #7467: `unplugin-auto-import` provides `shallowRef` globally, so
        // there is no `import { shallowRef } from 'vue'`. Clearing the ref with
        // `.value = undefined` is still the idiomatic Vue reset — not flagged.
        let src = "const userInfo = shallowRef();\n\
                   const logout = () => { userInfo.value = undefined; };";
        let pkg = r#"{"name":"app","devDependencies":{"unplugin-auto-import":"^0.17.0","vue":"^3.4.0"}}"#;
        assert!(run_in_project(src, pkg).is_empty());
    }

    #[test]
    fn allows_explicitly_imported_shallow_ref_value_undefined() {
        // The explicit-`vue`-import path (#4731) is preserved for `shallowRef`,
        // independent of `unplugin-auto-import`.
        let src = "import { shallowRef } from 'vue';\n\
                   const x = shallowRef();\n\
                   x.value = undefined;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_auto_imported_local_shadow_ref_value_undefined() {
        // A user-defined LOCAL `shallowRef` resolves to a local binding, so it is
        // not the auto-imported Vue global: `.value = undefined` stays flagged
        // even under `unplugin-auto-import`.
        let src = "const shallowRef = () => ({});\n\
                   const x = shallowRef();\n\
                   x.value = undefined;";
        let pkg = r#"{"name":"app","devDependencies":{"unplugin-auto-import":"^0.17.0"}}"#;
        assert_eq!(run_in_project(src, pkg).len(), 1);
    }

    #[test]
    fn flags_global_ref_value_undefined_without_auto_import() {
        // No `vue` import and no `unplugin-auto-import`: `shallowRef` is an unknown
        // global, so the Vue-ref exemption does not apply and the assignment flags.
        assert_eq!(
            run_on("const userInfo = shallowRef(); userInfo.value = undefined;").len(),
            1
        );
    }
}

//! react-no-destructure-zustand-store oxc backend.
//!
//! Flags `const { ... } = useStore()` (zero-argument store-hook call)
//! where the hook name matches the zustand convention `use*Store`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use std::sync::Arc;

pub struct Check;

fn is_store_hook_name(name: &str) -> bool {
    name.starts_with("use") && name.ends_with("Store") && name.len() > "useStore".len() - 1
}

/// True when `path`'s nearest `package.json` declares `zustand` in any
/// dependency section. This rule targets the Zustand whole-store destructuring
/// footgun, whose remedy is a per-field selector call (`useStore(s => s.x)`). A
/// project that does not depend on `zustand` has no Zustand stores: a
/// `use*Store()` name there is another library's hook — e.g. a Pinia store,
/// whose `useStore()` takes no selector and is destructured on purpose — so the
/// name-shape match must not fire. An unresolved manifest yields `false`, making
/// the rule a no-op: a Zustand-specific rule in a project of unknown provenance
/// is far likelier a false positive than a real hit.
fn project_depends_on_zustand(
    project: &crate::project::ProjectCtx,
    path: &std::path::Path,
) -> bool {
    project.nearest_package_json(path).is_some_and(|pkg| {
        pkg.dependencies.contains_key("zustand")
            || pkg.dev_dependencies.contains_key("zustand")
            || pkg.peer_dependencies.contains_key("zustand")
            || pkg.optional_dependencies.contains_key("zustand")
    })
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Store"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };

        // Pattern must be object destructuring.
        if !matches!(decl.id, BindingPattern::ObjectPattern(_)) {
            return;
        }

        // Init must be a call expression.
        let Some(Expression::CallExpression(call)) = &decl.init else {
            return;
        };

        // Callee must be a plain identifier.
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };

        let name = callee.name.as_str();
        if !is_store_hook_name(name) {
            return;
        }

        // Zero-argument call (no selector).
        if !call.arguments.is_empty() {
            return;
        }

        // Only a project that depends on `zustand` can have Zustand stores; in
        // any other project a `use*Store()` call is a different library's hook
        // (e.g. Pinia's `defineStore` result), where whole-store destructuring
        // is the idiomatic, correct usage.
        if !project_depends_on_zustand(ctx.project, ctx.path) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Destructuring the whole `{name}()` store — use a selector \
                 (e.g. `{name}(s => s.field)`) so the component re-renders \
                 only when that slice changes."
            ),
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
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Run the check on `source` written at `rel_path` next to a real
    /// `package.json` (`pkg_json`), resolving the `zustand`-dependency gate
    /// through `ProjectCtx::nearest_package_json`. The production
    /// `applies_to_file` gate (`skip_in_test_dir`) is applied first, so a
    /// test-directory path returns no diagnostics before the rule runs.
    fn run_with_pkg(pkg_json: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join(rel_path);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let lang = Language::from_path(&file_path).unwrap();
        let source_file = SourceFile { path: file_path.clone(), language: lang };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&file_path).unwrap();
        let file_ctx = FileCtx::build(&canon, source, lang, &project);
        if !super::super::META.applies_to_file(&file_ctx) {
            return vec![];
        }
        crate::rules::test_helpers::run_oxc_check(&Check, source, &canon, &project, &file_ctx)
    }

    #[test]
    fn flags_destructure_store_in_production() {
        let pkg = r#"{"dependencies":{"zustand":"^4"}}"#;
        let src = r#"const { count, inc } = useCounterStore();"#;
        let d = run_with_pkg(pkg, "src/Counter.tsx", src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_when_zustand_is_a_dev_dependency() {
        // The gate accepts `zustand` in any dependency section.
        let pkg = r#"{"devDependencies":{"zustand":"^4"}}"#;
        let src = r#"const { count, inc } = useCounterStore();"#;
        let d = run_with_pkg(pkg, "src/Counter.tsx", src);
        assert_eq!(d.len(), 1, "zustand in devDependencies must gate the rule in: {d:?}");
    }

    #[test]
    fn skips_destructure_store_in_test_file() {
        // Pattern from zustand's own test suite (issue #1346): even in a real
        // `zustand` project, a file under a test directory is exempt via the
        // rule's `skip_in_test_dir` gate.
        let pkg = r#"{"dependencies":{"zustand":"^4"}}"#;
        let src = r#"const { count, name } = useBoundStore();"#;
        assert!(run_with_pkg(pkg, "tests/persistAsync.test.tsx", src).is_empty());
    }

    #[test]
    fn skips_pinia_store_when_project_has_no_zustand() {
        // Issue #7363 (directus/directus): `useServerStore` is a Pinia store
        // (`defineStore` from `pinia`), not a Zustand store. The project depends
        // on `pinia`/`vue` and not `zustand`, so the whole-store destructure is
        // correct Pinia usage (reactivity via `storeToRefs()`) and the rule must
        // be a no-op — Pinia's `useServerStore()` takes no selector argument.
        let pkg = r#"{"dependencies":{"pinia":"^2","vue":"^3"}}"#;
        let src = r#"const { info: { queryLimit } } = useServerStore();"#;
        let d = run_with_pkg(pkg, "app/src/composables/use-page-size.ts", src);
        assert!(d.is_empty(), "Pinia store destructure must not flag: {d:?}");
    }

    #[test]
    fn flags_same_destructure_when_project_depends_on_zustand() {
        // Control for the FP above: the identical `use*Store()` destructure in a
        // project that actually depends on `zustand` still flags.
        let pkg = r#"{"dependencies":{"zustand":"^4"}}"#;
        let src = r#"const { info: { queryLimit } } = useServerStore();"#;
        let d = run_with_pkg(pkg, "app/src/composables/use-page-size.ts", src);
        assert_eq!(d.len(), 1, "zustand-dependency project must still flag: {d:?}");
    }
}

//! ts-no-restricted-types OXC backend.
//!
//! Flags banned types (`Function`) in type annotation positions by scanning
//! all TSTypeReference nodes in the semantic tree. Wrapper object types
//! (`Object`, `String`, `Number`, `Boolean`, `Symbol`, `BigInt`) are owned by
//! `ts-no-wrapper-object-types` and intentionally excluded here to avoid
//! duplicate diagnostics on the same type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use std::sync::Arc;

/// Banned type names and replacement messages.
const BANNED_TYPES: &[(&str, &str)] = &[(
    "Function",
    "Use a specific function type like `() => void` instead of `Function`.",
)];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // tsd type-test files pass banned types as inputs to the utility under
        // test (e.g. `ConditionalSimplify<SomeFunction, Function>`), so the
        // banned type is the test subject, not application code.
        if ctx.file.is_type_test_file() {
            return diagnostics;
        }

        for node in semantic.nodes().iter() {
            match node.kind() {
                // TSTypeReference with a single identifier name matching banned types.
                AstKind::TSTypeReference(type_ref) => {
                    let name = type_ref.type_name.to_string();
                    if let Some(&(_, msg)) = BANNED_TYPES.iter().find(|&&(t, _)| t == name.as_str())
                    {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, type_ref.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "ts-no-restricted-types".into(),
                            message: msg.to_string(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }

        diagnostics
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

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::FileCtx::build(
            std::path::Path::new(path),
            source,
            crate::files::Language::TypeScript,
            project,
        );
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &file)
    }

    #[test]
    fn flags_function_type() {
        let d = run_on("const f: Function = () => {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Function"));
    }

    #[test]
    fn ignores_object_wrapper_type() {
        // `Object` is owned by ts-no-wrapper-object-types; this rule must not
        // also flag it (regression for #1222).
        assert!(run_on("const o: Object = {};").is_empty());
    }

    #[test]
    fn exempts_tsd_type_test_file_issue3324() {
        // type-fest test-d/conditional-simplify.ts: `Function` is the input to
        // the utility under test, not application code.
        let src = "type SimplifiedFunctionPass = ConditionalSimplify<SomeFunction, Function>;";
        assert!(run_at(src, "test-d/conditional-simplify.ts").is_empty());
    }

    #[test]
    fn still_flags_function_type_in_production_issue3324() {
        assert_eq!(run_at("const f: Function = () => {};", "src/widget.ts").len(), 1);
    }
}

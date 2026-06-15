use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["isFinite"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Only the bare global call `isFinite(...)`; member calls like
        // `Number.isFinite(...)` or `foo.isFinite(...)` are clean. Unwrap
        // parentheses so `(isFinite)(...)` is still seen as a bare identifier.
        let Expression::Identifier(callee) = call.callee.get_inner_expression() else {
            return;
        };
        if callee.name != "isFinite" {
            return;
        }

        // A locally declared/imported/parameter `isFinite` shadows the global
        // and must not flag.
        if !semantic.is_reference_to_global_variable(callee) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, callee.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `Number.isFinite` instead of the global `isFinite`. The global coerces \
                      its argument; `Number.isFinite` does not."
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
    fn flags_global_is_finite_call() {
        let d = run_on("if (isFinite(x)) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-global-is-finite");
        assert!(d[0].message.contains("Number.isFinite"));
    }

    #[test]
    fn flags_global_is_finite_with_object_arg() {
        // Biome invalid fixture: `isFinite({})`.
        assert_eq!(run_on("isFinite({});").len(), 1);
    }

    #[test]
    fn flags_parenthesized_global() {
        // Biome invalid fixture: `(isFinite)({})`.
        assert_eq!(run_on("(isFinite)({});").len(), 1);
    }

    #[test]
    fn allows_number_is_finite() {
        // Biome valid fixture: `Number.isFinite(Number.NaN)`.
        assert!(run_on("Number.isFinite(Number.NaN);").is_empty());
    }

    #[test]
    fn allows_member_call() {
        assert!(run_on("foo.isFinite(x);").is_empty());
    }

    #[test]
    fn allows_global_this_member_call() {
        // `globalThis.isFinite(...)` is a member call, not a bare global.
        assert!(run_on("globalThis.isFinite({});").is_empty());
    }

    #[test]
    fn allows_shadowing_param() {
        // Biome valid fixture: a parameter named `isFinite` shadows the global.
        assert!(run_on("function localIsFinite(isFinite) {\n    isFinite({});\n}").is_empty());
    }

    #[test]
    fn allows_shadowing_local_var() {
        // Biome valid fixture: a local `var isFinite` shadows the global.
        assert!(run_on("function localVar() {\n    var isFinite;\n    isFinite()\n}").is_empty());
    }

    #[test]
    fn allows_shadowing_function_declaration() {
        assert!(run_on("function isFinite() {}\nisFinite(1);").is_empty());
    }

    #[test]
    fn allows_imported_is_finite() {
        assert!(run_on("import { isFinite } from './num';\nisFinite(1);").is_empty());
    }
}

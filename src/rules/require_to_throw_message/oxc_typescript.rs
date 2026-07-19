//! require-to-throw-message — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::test_validation_rejection::is_validation_rejection_subject;
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toThrow"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let name = member.property.name.as_str();
        if name != "toThrow" && name != "toThrowError" {
            return;
        }

        // Skip `.not.toThrow()` / `.not.toThrowError()` — asserts no error; no argument needed
        if let Expression::StaticMemberExpression(obj_member) = &member.object {
            if obj_member.property.name.as_str() == "not" {
                return;
            }
        }

        // Flag only when called with zero arguments.
        if !call.arguments.is_empty() {
            return;
        }

        // Skip schema-validation rejection tests: the expect() subject is a
        // callback that invokes a validation method (e.g. `schema.parse`).
        if is_validation_rejection_subject(&member.object, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Provide expected error message to toThrow().".into(),
            severity: Severity::Error,
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

    // Regression for issue #993 — zod's own test suite (relative imports, so
    // exemption must be shape-based, not import-based).
    #[test]
    fn exempts_zod_parse_rejection() {
        let d = run_on(
            r#"expect(() => stringSchema.parse({ constructor: 123, key: "value" })).toThrow();"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_parse_async_rejection() {
        let d = run_on("expect(() => schema.parseAsync(x)).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_validate_rejection() {
        let d = run_on("expect(() => schema.validate(x)).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_block_bodied_callback() {
        let d = run_on("expect(() => { schema.parse(x); }).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_awaited_parse_async_behind_rejects() {
        let d = run_on("expect(async () => { await schema.parseAsync(x); }).rejects.toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_bare_call_in_callback() {
        let d = run_on("expect(() => doStuff()).toThrow();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_non_validation_member_call() {
        let d = run_on("expect(() => service.compute()).toThrow();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_non_callback_subject() {
        let d = run_on("expect(myThrowingFn).toThrow();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn skips_not_to_throw() {
        let d = run_on("expect(() => doStuff()).not.toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_to_throw_with_message() {
        let d = run_on(r#"expect(() => doStuff()).toThrow("boom");"#);
        assert!(d.is_empty());
    }
}

//! test-check-exception OXC backend — flag `.toThrow()` with no arguments in test files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::test_validation_rejection::is_validation_rejection_subject;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

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
        if !is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Callee must be a member expression with property "toThrow"
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "toThrow" {
            return;
        }
        // Skip `.not.toThrow()` — asserts no error is thrown; no argument needed or meaningful
        if let oxc_ast::ast::Expression::StaticMemberExpression(obj_member) = &member.object {
            if obj_member.property.name.as_str() == "not" {
                return;
            }
        }
        // Arguments must be empty
        if !call.arguments.is_empty() {
            return;
        }
        // Skip schema-validation rejection tests: `expect(() => schema.parse(x))`
        // asserts the binary rejection contract — the error type is guaranteed by
        // the library, so naming it is boilerplate noise (issue #1338).
        if is_validation_rejection_subject(&member.object, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.toThrow()` without specifying error type or message — any error will pass."
                .into(),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.test.ts")
    }

    // Regression for issue #1338 — zod's own test suite uses bare `.toThrow()`
    // to assert the rejection contract; the error type is guaranteed by the
    // library, so naming it is boilerplate noise.
    #[test]
    fn exempts_zod_parse_rejection() {
        let d = run_on("expect(() => z.parse(a, 123)).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_schema_parse_rejection() {
        let d = run_on("expect(() => schema.parse(x)).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_validate_rejection() {
        let d = run_on("expect(() => schema.validate(x)).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_block_bodied_parse_rejection() {
        let d = run_on("expect(() => { schema.parse(x); }).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_awaited_parse_async_behind_rejects() {
        let d = run_on("expect(async () => { await schema.parseAsync(x); }).rejects.toThrow();");
        assert!(d.is_empty());
    }

    // The genuinely-weak cases the rule guards against stay flagged: a bare
    // `.toThrow()` whose subject is not a schema-validation callback passes for
    // *any* error, including the wrong one.
    #[test]
    fn flags_bare_callback() {
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

    // A `.rejects.toThrow()` whose subject is a bare identifier (e.g. a database
    // write thenable) carries no AST signal that its rejection is contractual,
    // so it stays flagged — there is no defensible discriminator for it.
    #[test]
    fn flags_rejects_on_bare_identifier() {
        let d = run_on("await expect(insert).rejects.toThrow();");
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

    #[test]
    fn ignores_non_test_files() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "expect(() => doStuff()).toThrow();",
            "t.ts",
        );
        assert!(d.is_empty());
    }
}

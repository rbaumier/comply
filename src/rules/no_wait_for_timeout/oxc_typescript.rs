use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("_test.")
        || s.contains(".e2e.")
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["waitForTimeout"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };
        if mem.property.name.as_str() != "waitForTimeout" {
            return;
        }
        // Anchor on the property name to match legacy behaviour.
        let (line, column) =
            byte_offset_to_line_col(ctx.source, mem.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`waitForTimeout` is a fixed sleep — replace with a \
                      web-first assertion or `waitForResponse`."
                .into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_wait_for_timeout_in_test() {
        let diags = run("login.test.ts", "await page.waitForTimeout(1000);");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-wait-for-timeout");
    }

    #[test]
    fn flags_in_spec_file() {
        let diags = run("checkout.spec.ts", "  await page.waitForTimeout(500);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_in_e2e_file() {
        let diags = run("smoke.e2e.ts", "await page.waitForTimeout(2000);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_wait_for_response() {
        let diags = run("api.test.ts", "await page.waitForResponse('**/api/data');");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run("helpers.ts", "await page.waitForTimeout(1000);");
        assert!(diags.is_empty());
    }

    #[test]
    fn correct_line_and_column() {
        let source = "const x = 1;\nawait page.waitForTimeout(300);\n";
        let diags = run("foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].column, 12);
    }
}

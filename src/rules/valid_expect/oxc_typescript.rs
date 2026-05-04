use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let is_expect = match &call.callee {
            Expression::Identifier(id) => id.name.as_str() == "expect",
            Expression::StaticMemberExpression(member) => {
                member.property.name.as_str() == "expect"
            }
            _ => false,
        };
        if !is_expect {
            return;
        }

        if !call.arguments.is_empty() {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`expect()` must be called with at least one argument.".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_empty_expect() {
        let d = run_on("expect().toBe(1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bare_expect() {
        let d = run_on("expect();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_expect_with_arg() {
        assert!(run_on("expect(value).toBe(1);").is_empty());
    }

    #[test]
    fn allows_expect_with_expression() {
        assert!(run_on("expect(1 + 2).toBe(3);").is_empty());
    }

    #[test]
    fn allows_non_expect_call() {
        assert!(run_on("something();").is_empty());
    }

    #[test]
    fn flags_member_expect() {
        let d = run_on("test.expect().toBe(1);");
        assert_eq!(d.len(), 1);
    }
}

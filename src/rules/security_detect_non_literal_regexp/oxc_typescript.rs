//! security-detect-non-literal-regexp oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new RegExp"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };
        let Expression::Identifier(callee) = &new_expr.callee else {
            return;
        };
        if callee.name.as_str() != "RegExp" {
            return;
        }
        let Some(first_arg) = new_expr.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };
        let is_static = match expr {
            Expression::StringLiteral(_) | Expression::RegExpLiteral(_) => true,
            Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
            _ => false,
        };
        if is_static {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new RegExp(<dynamic>)` lets user input drive the pattern — \
                      ReDoS / regex injection vector. Escape the input or use a \
                      static literal."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_dynamic_regexp() {
        let src = r#"const r = new RegExp(userInput);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_static_regexp() {
        let src = r#"const r = new RegExp("^foo$");"#;
        assert!(run(src).is_empty());
    }
}

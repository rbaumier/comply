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
        Some(&["postMessage"])
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
        // Callee must be a member expression: *.postMessage(...)
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "postMessage" {
            return;
        }
        // The target origin is the second argument (index 1).
        let Some(origin_arg) = call.arguments.get(1) else {
            return;
        };
        let Some(expr) = origin_arg.as_expression() else {
            return;
        };
        let Expression::StringLiteral(lit) = expr else {
            return;
        };
        if lit.value.as_str() != "*" {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`postMessage` with `\"*\"` target origin — specify an explicit origin."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_double_quote_star() {
        assert_eq!(run(r#"window.postMessage(data, "*");"#).len(), 1);
    }


    #[test]
    fn flags_single_quote_star() {
        assert_eq!(run("iframe.contentWindow.postMessage(msg, '*');").len(), 1);
    }


    #[test]
    fn allows_specific_origin() {
        assert!(run(r#"window.postMessage(data, "https://example.com");"#).is_empty());
    }


    #[test]
    fn allows_variable_origin() {
        assert!(run("window.postMessage(data, targetOrigin);").is_empty());
    }
}

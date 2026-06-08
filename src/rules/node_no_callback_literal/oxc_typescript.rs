//! node-no-callback-literal oxc backend — flag `cb('string')` patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

const CALLBACK_NAMES: &[&str] = &["cb", "callback", "next"];

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
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if !CALLBACK_NAMES.contains(&callee.name.as_str()) {
            return;
        }

        // Check if the first argument is a string literal or template literal.
        let Some(first) = call.arguments.first() else {
            return;
        };
        let is_string = matches!(
            first,
            Argument::StringLiteral(_) | Argument::TemplateLiteral(_)
        );
        if !is_string {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected string literal in error position of callback. Pass `new Error(...)` or `null` instead.".into(),
            severity: Severity::Warning,
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
    fn flags_cb_with_single_quote_string() {
        assert_eq!(run_on("cb('something went wrong');").len(), 1);
    }


    #[test]
    fn flags_callback_with_double_quote_string() {
        assert_eq!(run_on(r#"callback("error occurred");"#).len(), 1);
    }


    #[test]
    fn flags_next_with_string() {
        assert_eq!(run_on("next('fail');").len(), 1);
    }


    #[test]
    fn allows_cb_with_error_object() {
        assert!(run_on("cb(new Error('oops'));").is_empty());
    }


    #[test]
    fn allows_cb_with_null() {
        assert!(run_on("cb(null, data);").is_empty());
    }


    #[test]
    fn allows_cb_with_variable() {
        assert!(run_on("cb(err);").is_empty());
    }
}

//! OxcCheck backend — flag `.postMessage(data)` missing `targetOrigin`.

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
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Callee must be `*.postMessage`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "postMessage" {
            return;
        }
        // Must have exactly 1 argument (data, no targetOrigin).
        // 0 means no data either, 2+ means origin is provided.
        if call.arguments.len() != 1 {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`postMessage()` called without `targetOrigin` \u{2014} provide an explicit origin.".into(),
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
    fn flags_single_arg_post_message() {
        assert_eq!(run_on("window.postMessage(data);").len(), 1);
    }


    #[test]
    fn flags_self_post_message() {
        assert_eq!(run_on("self.postMessage(message);").len(), 1);
    }


    #[test]
    fn allows_post_message_with_origin() {
        assert!(run_on(r#"window.postMessage(data, "https://example.com");"#).is_empty());
    }


    #[test]
    fn allows_post_message_with_star() {
        assert!(run_on(r#"window.postMessage(data, '*');"#).is_empty());
    }


    #[test]
    fn flags_nested_call_single_arg() {
        assert_eq!(run_on("window.postMessage(getData());").len(), 1);
    }
}

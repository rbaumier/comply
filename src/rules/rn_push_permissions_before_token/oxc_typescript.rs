//! rn-push-permissions-before-token — OXC backend.
//! Flag `getExpoPushTokenAsync()` when no preceding `requestPermissionsAsync`
//! exists in the enclosing function.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["getExpoPushTokenAsync"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Check callee ends with `getExpoPushTokenAsync`.
        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if !callee_text.ends_with("getExpoPushTokenAsync") {
            return;
        }

        // Find the enclosing function body span.
        let token_start = call.span.start;
        let mut fn_body_start = None;
        let mut fn_body_end = None;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::Function(f) => {
                    if let Some(body) = &f.body {
                        fn_body_start = Some(body.span.start);
                        fn_body_end = Some(body.span.end);
                    }
                    break;
                }
                AstKind::ArrowFunctionExpression(f) => {
                    fn_body_start = Some(f.body.span.start);
                    fn_body_end = Some(f.body.span.end);
                    break;
                }
                _ => {}
            }
        }

        let Some(body_start) = fn_body_start else { return };
        let Some(body_end) = fn_body_end else { return };

        // Check if `requestPermissionsAsync` appears BEFORE the token call
        // in the function body source text.
        let body_src = &ctx.source[body_start as usize..body_end as usize];
        let token_offset_in_body = (token_start - body_start) as usize;

        let before_token = &body_src[..token_offset_in_body];
        if before_token.contains("requestPermissionsAsync") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`getExpoPushTokenAsync` without a preceding `requestPermissionsAsync` — request notification permissions first.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }

    #[test]
    fn flags_missing_permissions() {
        let src = "async function reg() { const t = await Notifications.getExpoPushTokenAsync(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_permissions() {
        let src = "async function reg() { await Notifications.requestPermissionsAsync(); const t = await Notifications.getExpoPushTokenAsync(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_when_permissions_come_after_token() {
        let src = "async function reg() { const t = await Notifications.getExpoPushTokenAsync(); await Notifications.requestPermissionsAsync(); }";
        assert_eq!(run(src).len(), 1);
    }
}

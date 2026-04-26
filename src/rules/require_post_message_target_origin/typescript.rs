//! require-post-message-target-origin AST backend.
//!
//! Flags `.postMessage(data)` calls missing the `targetOrigin` second argument.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // callee must be `*.postMessage`
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "postMessage" {
        return;
    }

    // arguments: must have exactly one argument (data, no targetOrigin)
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let arg_count = args.named_child_count();
    // Must have exactly 1 argument — 0 means no data either, 2+ means origin is provided
    if arg_count != 1 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "require-post-message-target-origin".into(),
        message: "`postMessage()` called without `targetOrigin` \u{2014} provide an explicit origin.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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

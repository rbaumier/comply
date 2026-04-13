//! node-no-callback-literal backend — flag `cb('string')` patterns.

use crate::diagnostic::{Diagnostic, Severity};

const CALLBACK_NAMES: &[&str] = &["cb", "callback", "next"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    let name = callee.utf8_text(source).unwrap_or("");
    if !CALLBACK_NAMES.contains(&name) {
        return;
    }

    // Check if the first argument is a string literal.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if !matches!(first.kind(), "string" | "template_string") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-callback-literal".into(),
        message: "Unexpected string literal in error position of callback. Pass `new Error(...)` or `null` instead.".into(),
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

//! no-confidential-logging — flag logging calls that contain
//! sensitive identifiers (password, token, apiKey, etc.).
//!
//! Matches `call_expression` nodes where the callee is a
//! `console.log/info/warn/error` or `logger.*` member expression,
//! and any argument's text contains a sensitive word.

use crate::diagnostic::{Diagnostic, Severity};

const CONSOLE_METHODS: &[&str] = &["log", "info", "warn", "error", "debug"];

const SENSITIVE_WORDS: &[&str] = &[
    "password",
    "secret",
    "token",
    "apikey",
    "api_key",
    "authorization",
    "credential",
    "ssn",
    "creditcard",
    "credit_card",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };

    if !is_logging_callee(&callee, source) {
        return;
    }

    // Check all arguments for sensitive words
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Ok(args_text) = args.utf8_text(source) else { return };
    let lower = args_text.to_ascii_lowercase();

    if !SENSITIVE_WORDS.iter().any(|w| lower.contains(w)) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-confidential-logging".into(),
        message: "Logging call contains sensitive data — redact secrets before logging.".into(),
        severity: Severity::Error,
    });
}

fn is_logging_callee(node: &tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = node.child_by_field_name("object") else { return false };
    let Some(prop) = node.child_by_field_name("property") else { return false };
    let Ok(obj_text) = obj.utf8_text(source) else { return false };
    let Ok(prop_text) = prop.utf8_text(source) else { return false };

    // console.log/info/warn/error/debug
    if obj_text == "console" && CONSOLE_METHODS.contains(&prop_text) {
        return true;
    }

    // logger.*
    if obj_text == "logger" {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_console_log_with_password() {
        assert_eq!(run_on("console.log('password:', userPassword);").len(), 1);
    }

    #[test]
    fn flags_console_error_with_token() {
        assert_eq!(run_on("console.error(`token=${token}`);").len(), 1);
    }

    #[test]
    fn flags_logger_with_api_key() {
        assert_eq!(run_on("logger.info('apiKey:', key);").len(), 1);
    }

    #[test]
    fn allows_logging_without_sensitive_data() {
        assert!(run_on("console.log('User logged in');").is_empty());
    }

    #[test]
    fn allows_non_logging_with_sensitive_word() {
        assert!(run_on("const password = getPassword();").is_empty());
    }
}

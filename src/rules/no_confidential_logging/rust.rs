//! no-confidential-logging Rust backend — flag logging macros that contain
//! sensitive identifiers (password, token, api_key, etc.).
//!
//! Matches `macro_invocation` nodes where the macro is `log::info!`,
//! `tracing::warn!`, `println!`, `eprintln!`, etc., and the arguments
//! text contains a sensitive word.

use crate::diagnostic::{Diagnostic, Severity};

const LOG_MACROS: &[&str] = &[
    "log::trace",
    "log::debug",
    "log::info",
    "log::warn",
    "log::error",
    "tracing::trace",
    "tracing::debug",
    "tracing::info",
    "tracing::warn",
    "tracing::error",
    "trace",
    "debug",
    "info",
    "warn",
    "error",
    "println",
    "eprintln",
];

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
    if node.kind() != "macro_invocation" {
        return;
    }

    // Get macro name from the `macro` field.
    let Some(macro_node) = node.child_by_field_name("macro") else { return };
    let Ok(macro_name) = macro_node.utf8_text(source) else { return };

    // Strip trailing `!` if present in the node text.
    let name = macro_name.trim_end_matches('!');
    if !LOG_MACROS.contains(&name) {
        return;
    }

    // Check all token_tree (arguments) for sensitive words.
    let Ok(full_text) = node.utf8_text(source) else { return };
    let lower = full_text.to_ascii_lowercase();

    if !SENSITIVE_WORDS.iter().any(|w| lower.contains(w)) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-confidential-logging".into(),
        message: "Logging call contains sensitive data \u{2014} redact secrets before logging.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_println_with_password() {
        assert_eq!(run_on(r#"fn f() { println!("password: {}", password); }"#).len(), 1);
    }

    #[test]
    fn flags_log_info_with_token() {
        assert_eq!(
            run_on(r#"fn f() { log::info!("user token: {}", token); }"#).len(),
            1
        );
    }

    #[test]
    fn allows_logging_without_sensitive_data() {
        assert!(run_on(r#"fn f() { println!("User logged in"); }"#).is_empty());
    }

    #[test]
    fn allows_non_logging_with_sensitive_word() {
        assert!(run_on(r#"fn f() { let password = get_password(); }"#).is_empty());
    }
}

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

const BOOLEAN_PREFIXES: &[&str] = &["has_", "is_", "no_", "without_", "needs_", "can_", "should_"];

fn is_boolean_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    BOOLEAN_PREFIXES.iter().any(|p| lower.starts_with(p))
}

fn has_sensitive_identifier(node: tree_sitter::Node, source: &[u8]) -> bool {
    let kind = node.kind();
    if kind == "string_literal" || kind == "raw_string_literal" || kind == "string_content" {
        return false;
    }
    if kind == "field_expression" {
        let mut cursor = node.walk();
        if let Some(field) = node.children(&mut cursor).last() {
            if field.kind() == "field_identifier" {
                let Ok(field_name) = field.utf8_text(source) else { return false };
                let lower = field_name.to_ascii_lowercase();
                return SENSITIVE_WORDS.iter().any(|w| lower.contains(w));
            }
        }
        return false;
    }
    if kind == "identifier" || kind == "field_identifier" {
        let Ok(text) = node.utf8_text(source) else { return false };
        if is_boolean_name(text) {
            return false;
        }
        if let Some(next) = node.next_sibling() {
            if next.utf8_text(source).is_ok_and(|t| t == ".") {
                return false;
            }
        }
        let lower = text.to_ascii_lowercase();
        if SENSITIVE_WORDS.iter().any(|w| lower.contains(w)) {
            return true;
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_sensitive_identifier(child, source) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    let Some(macro_node) = node.child_by_field_name("macro") else { return };
    let Ok(macro_name) = macro_node.utf8_text(source) else { return };

    let name = macro_name.trim_end_matches('!');
    if !LOG_MACROS.contains(&name) {
        return;
    }

    let mut cursor = node.walk();
    let Some(token_tree) = node.children(&mut cursor).find(|c| c.kind() == "token_tree") else {
        return;
    };

    if !has_sensitive_identifier(token_tree, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
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

    #[test]
    fn allows_sensitive_word_only_in_format_string() {
        assert!(run_on(r#"fn f() { error!("Could not create token: {e}"); }"#).is_empty());
    }

    #[test]
    fn allows_descriptive_error_about_secret() {
        assert!(run_on(r#"fn f() { error!("Error creating Biscuit from application secret: {e}"); }"#).is_empty());
    }

    #[test]
    fn flags_interpolated_secret_variable() {
        assert_eq!(run_on(r#"fn f() { info!("value: {}", api_key); }"#).len(), 1);
    }

    #[test]
    fn allows_boolean_has_secret() {
        assert!(run_on(r#"fn f() { println!("Auth: {}", if has_secret { "Yes" } else { "No" }); }"#).is_empty());
    }

    #[test]
    fn allows_boolean_is_token_valid() {
        assert!(run_on(r#"fn f() { info!("valid: {}", is_token_valid); }"#).is_empty());
    }

    #[test]
    fn allows_non_sensitive_field_on_secret_struct() {
        assert!(run_on(r#"fn f() { debug!("id: {}", application_secret.application_id); }"#).is_empty());
    }

    #[test]
    fn flags_sensitive_field_access() {
        assert_eq!(run_on(r#"fn f() { info!("val: {}", config.api_key); }"#).len(), 1);
    }
}

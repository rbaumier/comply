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

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };

    if !is_logging_callee(&callee, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    if !has_sensitive_identifier(&args, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-confidential-logging".into(),
        message: "Logging call contains sensitive data — redact secrets before logging.".into(),
        severity: Severity::Error,
        span: None,
    });
}

fn has_sensitive_identifier(args: &tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        match child.kind() {
            "string" => continue,
            "template_string" => {
                let mut tc = child.walk();
                for part in child.children(&mut tc) {
                    if part.kind() == "template_substitution" {
                        let Ok(text) = part.utf8_text(source) else {
                            continue;
                        };
                        let lower = text.to_ascii_lowercase();
                        if SENSITIVE_WORDS.iter().any(|w| lower.contains(w)) {
                            return true;
                        }
                    }
                }
            }
            _ => {
                let Ok(text) = child.utf8_text(source) else {
                    continue;
                };
                let lower = text.to_ascii_lowercase();
                if SENSITIVE_WORDS.iter().any(|w| lower.contains(w)) {
                    return true;
                }
            }
        }
    }
    false
}

fn is_logging_callee(node: &tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = node.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = node.child_by_field_name("property") else {
        return false;
    };
    let Ok(obj_text) = obj.utf8_text(source) else {
        return false;
    };
    let Ok(prop_text) = prop.utf8_text(source) else {
        return false;
    };

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
        assert_eq!(run_on("logger.info('label:', apiKey);").len(), 1);
    }

    #[test]
    fn allows_logging_without_sensitive_data() {
        assert!(run_on("console.log('User logged in');").is_empty());
    }

    #[test]
    fn allows_non_logging_with_sensitive_word() {
        assert!(run_on("const password = getPassword();").is_empty());
    }

    #[test]
    fn allows_string_literal_mentioning_token() {
        assert!(run_on(r#"console.log("Token refresh succeeded");"#).is_empty());
    }

    #[test]
    fn allows_descriptive_message_about_tokens() {
        assert!(run_on(r#"logger.info("Start cleaning up expired tokens...");"#).is_empty());
    }

    #[test]
    fn flags_identifier_token_variable() {
        assert_eq!(run_on("console.log(userToken);").len(), 1);
    }
}

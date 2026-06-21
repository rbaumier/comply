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

const BOOLEAN_PREFIXES: &[&str] = &[
    "has_", "is_", "no_", "without_", "needs_", "can_", "should_",
];

/// Suffixes that indicate the identifier holds metadata *about* a secret
/// (e.g. a filesystem path, directory name, or container) rather than the
/// secret value itself. `token_path` is a file path; `credential_dir` is a
/// directory — neither leaks a credential.
const METADATA_SUFFIXES: &[&str] = &["_path", "_dir", "_file", "_store", "_cache"];

/// Word segments that mark an identifier as describing a count, position, or
/// category *about* a secret-named value rather than the value itself. In a
/// parser/lexer/compiler, `token` denotes a grammar terminal and `token_index`
/// is its position in the token stream — numeric metadata, not a credential.
/// Logging a secret's index/count/length/kind cannot leak the secret.
/// `index`/`count`/`length`/`position`/`offset` are clearly numeric; `kind`/
/// `type` name a category enum, never the value.
const METADATA_QUALIFIERS: &[&str] = &[
    "index", "idx", "count", "length", "len", "position", "pos", "offset", "kind", "type",
];

fn is_boolean_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    BOOLEAN_PREFIXES.iter().any(|p| lower.starts_with(p))
}

/// Returns true when `name` follows Rust's SCREAMING_SNAKE_CASE const
/// convention (at least one uppercase letter, no lowercase letter, len >= 2).
/// Any such name is treated as a compile-time constant and exempted: a runtime
/// secret is bound to a `snake_case`/`camelCase` variable or field by Rust
/// convention, while an all-uppercase name is a literal limit or flag — e.g.
/// `MAX_TOKEN_LEN`, which holds a token's length bound, not its value.
fn is_const_name(name: &str) -> bool {
    name.len() >= 2
        && name.bytes().any(|b| b.is_ascii_uppercase())
        && !name.bytes().any(|b| b.is_ascii_lowercase())
}

/// Returns true when `name` contains a sensitive word only because it is a
/// metadata identifier (e.g. `token_path` where `token` is followed by a
/// metadata suffix). Such identifiers hold filesystem paths or containers,
/// not secret values.
fn is_metadata_only(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_WORDS.iter().any(|w| {
        if let Some(after) = lower.strip_prefix(*w) {
            METADATA_SUFFIXES.iter().any(|s| after == *s || after.starts_with(s))
        } else {
            false
        }
    })
}

/// Splits an identifier into lowercase word segments across both snake_case
/// (`_`) and camelCase/PascalCase boundaries, so `opt_token_index` and
/// `optTokenIndex` both yield `["opt", "token", "index"]`.
fn word_segments(name: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for c in name.chars() {
        if c == '_' {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
        } else if c.is_ascii_uppercase() && !current.is_empty() {
            segments.push(std::mem::take(&mut current));
            current.push(c.to_ascii_lowercase());
        } else {
            current.push(c.to_ascii_lowercase());
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// Returns true when every sensitive word in `name` is immediately followed by
/// a metadata qualifier (`token_index`, `tokenCount`, `opt_token_index`, …),
/// making the identifier a count/position/category *about* the secret rather
/// than the secret value. Logging a secret's index or length leaks nothing.
///
/// The qualifier must FOLLOW the sensitive segment it describes: `token_index`
/// is "the index of the token" and is exempt, but a sensitive segment with no
/// trailing qualifier (e.g. `secret` in `max_token_len_secret`) is the value
/// itself and keeps the name flagged. The check is segment-aware (not a
/// substring scan), so `accessToken`/`auth_token` carry no qualifier and stay
/// flagged.
fn is_metadata_qualified(name: &str) -> bool {
    let segments = word_segments(name);
    let is_sensitive = |seg: &str| SENSITIVE_WORDS.iter().any(|w| seg.contains(w));
    let is_qualifier = |seg: &str| METADATA_QUALIFIERS.contains(&seg);

    let mut saw_sensitive = false;
    for (i, seg) in segments.iter().enumerate() {
        if is_sensitive(seg) {
            saw_sensitive = true;
            let qualified = segments
                .get(i + 1)
                .is_some_and(|next| is_qualifier(next.as_str()));
            if !qualified {
                return false;
            }
        }
    }
    saw_sensitive
}

fn has_sensitive_identifier(node: tree_sitter::Node, source: &[u8]) -> bool {
    let kind = node.kind();
    if kind == "string_literal" || kind == "raw_string_literal" || kind == "string_content" {
        return false;
    }
    if kind == "field_expression" {
        let mut cursor = node.walk();
        if let Some(field) = node.children(&mut cursor).last()
            && field.kind() == "field_identifier" {
                let Ok(field_name) = field.utf8_text(source) else {
                    return false;
                };
                if is_metadata_qualified(field_name) {
                    return false;
                }
                let lower = field_name.to_ascii_lowercase();
                return SENSITIVE_WORDS.iter().any(|w| lower.contains(w));
            }
        return false;
    }
    if kind == "identifier" || kind == "field_identifier" {
        let Ok(text) = node.utf8_text(source) else {
            return false;
        };
        if is_boolean_name(text) {
            return false;
        }
        if is_const_name(text) {
            return false;
        }
        if is_metadata_only(text) {
            return false;
        }
        if is_metadata_qualified(text) {
            return false;
        }
        if let Some(next) = node.next_sibling()
            && next.utf8_text(source).is_ok_and(|t| t == ".") {
                return false;
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
    if crate::rules::rust_helpers::is_in_test_context(node, source) {
        return;
    }

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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_println_with_password() {
        assert_eq!(
            run_on(r#"fn f() { println!("password: {}", password); }"#).len(),
            1
        );
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
        assert!(
            run_on(r#"fn f() { error!("Error creating Biscuit from application secret: {e}"); }"#)
                .is_empty()
        );
    }

    #[test]
    fn flags_interpolated_secret_variable() {
        assert_eq!(
            run_on(r#"fn f() { info!("value: {}", api_key); }"#).len(),
            1
        );
    }

    #[test]
    fn allows_boolean_has_secret() {
        assert!(
            run_on(r#"fn f() { println!("Auth: {}", if has_secret { "Yes" } else { "No" }); }"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_boolean_is_token_valid() {
        assert!(run_on(r#"fn f() { info!("valid: {}", is_token_valid); }"#).is_empty());
    }

    #[test]
    fn allows_non_sensitive_field_on_secret_struct() {
        assert!(
            run_on(r#"fn f() { debug!("id: {}", application_secret.application_id); }"#).is_empty()
        );
    }

    #[test]
    fn flags_sensitive_field_access() {
        assert_eq!(
            run_on(r#"fn f() { info!("val: {}", config.api_key); }"#).len(),
            1
        );
    }

    // Regression tests for issue #984: path/metadata variables are FPs
    #[test]
    fn allows_token_path_variable() {
        // `token_path` is a PathBuf pointing to a cache file — not a secret
        assert!(run_on(r#"
            fn f(token_path: &str) {
                debug!("Token cache file path {:?} does not exist", token_path);
            }
        "#).is_empty());
    }

    #[test]
    fn allows_credential_dir_variable() {
        assert!(run_on(r#"
            fn f() {
                debug!("Reading credentials from {:?}", credential_dir);
            }
        "#).is_empty());
    }

    #[test]
    fn allows_token_file_variable() {
        assert!(run_on(r#"
            fn f() {
                info!("Loading token file: {:?}", token_file);
            }
        "#).is_empty());
    }

    #[test]
    fn allows_secret_store_variable() {
        assert!(run_on(r#"
            fn f() {
                debug!("Using secret store at {:?}", secret_store);
            }
        "#).is_empty());
    }

    // Regression for issue #5120: in a LALR parser state machine, `token` is a
    // grammar terminal and `token_index` is its position in the token stream —
    // numeric metadata, not an auth token. Logging it leaks nothing.
    #[test]
    fn allows_token_index_in_parser() {
        assert!(run_on(r#"
            fn f(token_index: usize) {
                debug!("\\ token_index: {:?}", token_index);
            }
        "#).is_empty());
    }

    #[test]
    fn allows_opt_token_index_in_parser() {
        assert!(run_on(r#"
            fn f(opt_lookahead: usize, opt_token_index: Option<usize>) {
                debug!(
                    "\\+ error_recovery(opt_lookahead={:?}, opt_token_index={:?})",
                    opt_lookahead, opt_token_index,
                );
            }
        "#).is_empty());
    }

    #[test]
    fn allows_camel_case_token_index() {
        assert!(run_on(r#"fn f(tokenIndex: usize) { debug!("{:?}", tokenIndex); }"#).is_empty());
    }

    #[test]
    fn allows_token_count_and_length() {
        assert!(run_on(r#"fn f(token_count: usize) { info!("n: {}", token_count); }"#).is_empty());
        assert!(run_on(r#"fn f(token_len: usize) { info!("n: {}", token_len); }"#).is_empty());
        assert!(run_on(r#"fn f(token_kind: u8) { info!("k: {:?}", token_kind); }"#).is_empty());
    }

    // `token_id` carries no numeric/category qualifier — an id can be the secret
    // value itself (a session/bearer token id), so it stays flagged.
    #[test]
    fn still_flags_token_id() {
        assert_eq!(
            run_on(r#"fn f(token_id: String) { debug!("id: {}", token_id); }"#).len(),
            1
        );
    }

    #[test]
    fn still_flags_token_variable() {
        // bare `token` is still a secret
        assert_eq!(
            run_on(r#"fn f() { debug!("token: {}", token); }"#).len(),
            1
        );
    }

    #[test]
    fn still_flags_auth_token_variable() {
        // `auth_token` does not have a metadata suffix after the sensitive word
        assert_eq!(
            run_on(r#"fn f() { debug!("auth: {}", auth_token); }"#).len(),
            1
        );
    }

    #[test]
    fn skips_cfg_test_module() {
        assert!(run_on(r#"
            #[cfg(test)]
            mod tests {
                fn check() { error!("token: {}", token); }
            }
        "#)
        .is_empty());
    }

    #[test]
    fn skips_test_fn() {
        assert!(run_on(r#"
            #[test]
            fn it_works() {
                info!("token: {}", api_key);
            }
        "#)
        .is_empty());
    }

    // Regression for issue #4773: the trigger was `MAX_TOKEN_LEN`, a
    // SCREAMING_SNAKE const whose name contains "token" — a compile-time length
    // bound, not a runtime secret.
    #[test]
    fn allows_logging_token_constant_with_len() {
        assert!(run_on(r#"
            fn f(token: &Token) {
                if token.text.len() > MAX_TOKEN_LEN {
                    warn!(
                        "A token exceeding MAX_TOKEN_LEN ({}>{}) was dropped.",
                        token.text.len(),
                        MAX_TOKEN_LEN
                    );
                }
            }
        "#).is_empty());
    }

    #[test]
    fn allows_screaming_snake_const_with_sensitive_word() {
        assert!(run_on(r#"fn f() { warn!("limit: {}", MAX_TOKEN_LEN); }"#).is_empty());
    }

    // A lowercase secret variable is still flagged — the const exemption keys
    // strictly on SCREAMING_SNAKE casing.
    #[test]
    fn still_flags_lowercase_secret_const_name() {
        assert_eq!(
            run_on(r#"fn f() { warn!("limit: {}", max_token_len_secret); }"#).len(),
            1
        );
    }
}

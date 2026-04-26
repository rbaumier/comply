//! no-insecure-jwt backend for Rust.
//!
//! Flags weak JWT algorithms (`none`, `HS256`) in Rust code.
//! Detects string literals and identifiers referencing insecure algorithms.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["string_literal", "raw_string_literal"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    let lower = text.to_ascii_lowercase();

    // algorithm: "none"
    if lower.contains("\"none\"") || lower == "\"none\"" {
        // Check if this is in a JWT context by looking at surrounding source
        let line_start = node.start_position().row;
        let full_text = std::str::from_utf8(source).unwrap_or("");
        let line = full_text.lines().nth(line_start).unwrap_or("");
        let line_lower = line.to_ascii_lowercase();
        if line_lower.contains("algorithm") || line_lower.contains("jwt") || line_lower.contains("alg") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "no-insecure-jwt",
                "Insecure JWT algorithm `none` — use RS256 or ES256.".into(),
                Severity::Error,
            ));
            return;
        }
    }

    // HS256 in JWT context
    if lower.contains("hs256") {
        let line_start = node.start_position().row;
        let full_text = std::str::from_utf8(source).unwrap_or("");
        let line = full_text.lines().nth(line_start).unwrap_or("");
        let line_lower = line.to_ascii_lowercase();
        if line_lower.contains("jwt") || line_lower.contains("algorithm") || line_lower.contains("alg") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "no-insecure-jwt",
                "HS256 in JWT context — prefer asymmetric algorithms (RS256, ES256).".into(),
                Severity::Error,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_algorithm_none() {
        assert_eq!(
            run_on(r#"fn f() { let alg = Algorithm::from("none"); }"#).len(),
            1,
        );
    }

    #[test]
    fn flags_hs256_in_jwt_context() {
        assert_eq!(
            run_on(r#"fn f() { let jwt_alg = "HS256"; }"#).len(),
            1,
        );
    }

    #[test]
    fn allows_rs256() {
        assert!(run_on(r#"fn f() { let jwt_alg = "RS256"; }"#).is_empty());
    }
}

//! no-ecb-mode backend for Rust.
//!
//! Flags ECB cipher mode in string literals — same detection as TS but
//! adapted to Rust's `string_literal` / `raw_string_literal` nodes.

use crate::diagnostic::{Diagnostic, Severity};

fn contains_ecb(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("-ecb") {
        return true;
    }
    if lower.contains("_ecb") {
        return true;
    }
    if lower.contains(".ecb") {
        return true;
    }
    // Strip quotes and check bare "ecb"
    let inner = if text.len() >= 2 {
        &text[1..text.len() - 1]
    } else {
        text
    };
    inner.eq_ignore_ascii_case("ecb")
}

crate::ast_check! { on ["string_literal", "raw_string_literal"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return };
    if !contains_ecb(text) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-ecb-mode".into(),
        message: "ECB cipher mode is insecure — use CBC, CTR, or GCM instead.".into(),
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
    fn flags_aes_ecb() {
        assert_eq!(run_on(r#"fn f() { let mode = "aes-128-ecb"; }"#).len(), 1);
    }

    #[test]
    fn flags_ecb_mode_constant() {
        assert_eq!(run_on(r#"fn f() { let mode = "ECB"; }"#).len(), 1);
    }

    #[test]
    fn allows_cbc_mode() {
        assert!(run_on(r#"fn f() { let mode = "aes-128-cbc"; }"#).is_empty());
    }

    #[test]
    fn allows_gcm_mode() {
        assert!(run_on(r#"fn f() { let mode = "aes-256-gcm"; }"#).is_empty());
    }
}

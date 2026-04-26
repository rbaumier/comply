//! detect-option-rejectunauthorized backend — flag
//! `{ rejectUnauthorized: false }` object properties.

use crate::diagnostic::{Diagnostic, Severity};

fn key_text<'a>(key: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    // Key can be a plain property_identifier or a string literal.
    let text = key.utf8_text(source).unwrap_or("");
    text.trim_matches(|c| c == '"' || c == '\'')
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let Some(value) = node.child_by_field_name("value") else { return };
    if key_text(key, source) != "rejectUnauthorized" {
        return;
    }
    if value.kind() != "false" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "detect-option-rejectunauthorized".into(),
        message: "`rejectUnauthorized: false` disables TLS certificate validation — remove it.".into(),
        severity: Severity::Error,
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
    fn flags_reject_unauthorized_false() {
        let source = "const opts = { rejectUnauthorized: false };";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_string_key() {
        let source = r#"const opts = { "rejectUnauthorized": false };"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_reject_unauthorized_true() {
        let source = "const opts = { rejectUnauthorized: true };";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_other_option_false() {
        let source = "const opts = { somethingElse: false };";
        assert!(run_on(source).is_empty());
    }
}

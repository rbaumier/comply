use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "color_value" { return; }
    let text = node.utf8_text(source).unwrap_or_default();
    let Some(hex) = text.strip_prefix('#') else { return; };
    let valid_len = matches!(hex.len(), 3 | 4 | 6 | 8);
    let all_hex = hex.chars().all(|c| c.is_ascii_hexdigit());
    if valid_len && all_hex { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Invalid hex color `{text}`; expected 3, 4, 6, or 8 hex digits."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_non_hex_chars() {
        assert_eq!(run(".a { color: #gg0000; }").len(), 1);
    }

    #[test]
    fn flags_wrong_length() {
        assert_eq!(run(".a { color: #12345; }").len(), 1);
    }

    #[test]
    fn allows_valid_six_digit_hex() {
        assert!(run(".a { color: #ff0000; }").is_empty());
    }

    #[test]
    fn allows_short_and_long_hex() {
        assert!(run(".a { color: #fff; }").is_empty());
        assert!(run(".a { color: #ff000080; }").is_empty());
    }
}

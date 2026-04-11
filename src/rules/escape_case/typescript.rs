//! escape-case — flag lowercase hex digits in escape sequences.
//!
//! Walks the AST looking for string and template_string nodes containing
//! escape sequences like `\xff`, `\u00ff`, `\u{ff}` and flags when hex
//! digits are lowercase. The fix is to uppercase: `\xFF`, `\u00FF`, `\u{FF}`.

use crate::diagnostic::{Diagnostic, Severity};
use regex::Regex;
use std::sync::LazyLock;

/// Matches escape sequences with hex digits: \xNN, \uNNNN, \u{N+}
static RE_ESCAPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(x[0-9A-Fa-f]{2}|u[0-9A-Fa-f]{4}|u\{[0-9A-Fa-f]+\})").unwrap()
});

crate::ast_check! { |node, source, ctx, diagnostics|
    if !matches!(node.kind(), "string" | "template_string") {
        return;
    }

    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };

    let base_line = node.start_position().row;
    let base_col = node.start_position().column;

    for mat in RE_ESCAPE.find_iter(text) {
        let matched = mat.as_str();
        let body = &matched[1..]; // everything after `\`

        if !has_lowercase_hex(body) {
            continue;
        }

        // Check that the backslash is not itself escaped.
        let prefix = &text[..mat.start()];
        let trailing_bs = prefix.len() - prefix.trim_end_matches('\\').len();
        if trailing_bs % 2 == 1 {
            continue;
        }

        let uppercased = format!("\\{}", uppercase_hex(body));

        // Calculate position within the node.
        let before_match = &text[..mat.start()];
        let newlines = before_match.matches('\n').count();
        let col = if newlines > 0 {
            before_match.len() - before_match.rfind('\n').unwrap() - 1
        } else {
            base_col + before_match.len()
        };

        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: base_line + newlines + 1,
            column: col + 1,
            rule_id: "escape-case".into(),
            message: format!(
                "Use uppercase characters for the value of the escape \
                 sequence: `{matched}` -> `{uppercased}`."
            ),
            severity: Severity::Warning,
        });
    }
}

fn has_lowercase_hex(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_ascii_lowercase() && c.is_ascii_hexdigit())
}

fn uppercase_hex(body: &str) -> String {
    body.chars()
        .map(|c| {
            if c.is_ascii_hexdigit() && c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_lowercase_hex_escape() {
        let d = run_on(r#"const a = "\xff";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\xFF"));
    }

    #[test]
    fn flags_lowercase_unicode_escape() {
        let d = run_on(r#"const a = "\u00ff";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\u00FF"));
    }

    #[test]
    fn flags_lowercase_unicode_brace_escape() {
        let d = run_on(r#"const a = "\u{1a2b}";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\u{1A2B}"));
    }

    #[test]
    fn allows_uppercase_escape() {
        assert!(run_on(r#"const a = "\xFF";"#).is_empty());
    }

    #[test]
    fn allows_uppercase_unicode() {
        assert!(run_on(r#"const a = "\u00FF";"#).is_empty());
    }

    #[test]
    fn flags_multiple_on_one_line() {
        let d = run_on(r#"const a = "\xff\u00ab";"#);
        assert_eq!(d.len(), 2);
    }
}

//! no-inverted-boolean-check Rust backend — flag `!a == b` patterns.
//!
//! In Rust `!` binds tighter than `==`, so `!a == b` is `(!a) == b`,
//! not `!(a == b)`. This is almost always a mistake.

use crate::diagnostic::{Diagnostic, Severity};

/// Detect patterns like `!identifier == expr` or `!identifier != expr`.
fn has_inverted_check(line: &str) -> bool {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'!' {
            // Make sure this is not `!=` (a comparison operator).
            if i + 1 < len && bytes[i + 1] == b'=' {
                i += 1;
                continue;
            }
            // Skip optional whitespace after `!`.
            let mut j = i + 1;
            while j < len && bytes[j] == b' ' {
                j += 1;
            }
            // Expect an identifier char (letter, _).
            if j < len && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                // Skip identifier.
                let mut k = j;
                while k < len
                    && (bytes[k].is_ascii_alphanumeric()
                        || bytes[k] == b'_'
                        || bytes[k] == b'.')
                {
                    k += 1;
                }
                // Skip whitespace.
                while k < len && bytes[k] == b' ' {
                    k += 1;
                }
                // Check for `==` or `!=` (but not `=>` which is match arm).
                if k + 1 < len
                    && (bytes[k] == b'=' || bytes[k] == b'!')
                    && bytes[k + 1] == b'='
                    && !(k + 2 < len && bytes[k + 2] == b'=')
                {
                    // Exclude `=>` (match arm arrow)
                    if bytes[k] == b'=' && bytes[k + 1] == b'>' {
                        i += 1;
                        continue;
                    }
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "source_file" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        if has_inverted_check(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-inverted-boolean-check".into(),
                message: "`!a == b` negates `a` before comparing \u{2014} use `a != b` or `!(a == b)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_not_a_equals_b() {
        assert_eq!(run_on("fn f(a: bool, b: bool) { if !a == b {} }").len(), 1);
    }

    #[test]
    fn flags_not_a_not_equals_b() {
        assert_eq!(run_on("fn f(a: bool, b: bool) { if !a != b {} }").len(), 1);
    }

    #[test]
    fn allows_normal_comparison() {
        assert!(run_on("fn f(a: i32, b: i32) { if a == b {} }").is_empty());
    }

    #[test]
    fn allows_negated_result() {
        assert!(run_on("fn f(a: i32, b: i32) { if !(a == b) {} }").is_empty());
    }
}

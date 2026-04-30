//! regex-prefer-char-class TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! single-character `|` chains inside string literals (URLs, scoped
//! imports, Tailwind classes) cannot false-positive as alternations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Returns true when `pattern` contains an alternation of three or more
/// single-character alternatives at the top level (e.g. `a|b|c`).
///
/// Parenthesised groups and character-class contents are skipped so an
/// alternation inside `[...]` or `(?:...)` doesn't count against the
/// surrounding regex.
fn has_single_char_alternation(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut in_class = false;
    // Walk the pattern, collecting stretches of top-level tokens between
    // `|` separators. A "single char" token is exactly one non-special
    // byte (or one escape like `\d`) -- we conservatively require one
    // plain ASCII alphanumeric so we only flag the classic `a|b|c` case.
    let mut run: Vec<Option<u8>> = Vec::new();
    // `current` tracks the token between pipes: `Some(c)` after one
    // plain byte, `None` once the token is disqualified (too long,
    // escape, group, etc.).
    let mut current: Option<Option<u8>> = None;

    let flush = |run: &mut Vec<Option<u8>>, current: &mut Option<Option<u8>>| {
        if let Some(tok) = current.take() {
            run.push(tok);
        }
    };

    let alternation_hit =
        |run: &[Option<u8>]| -> bool { run.len() >= 3 && run.iter().all(|t| t.is_some()) };

    while i < bytes.len() {
        let b = bytes[i];
        if in_class {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if b == b']' {
                in_class = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'\\' if i + 1 < bytes.len() => {
                // Any escape disqualifies the current token.
                current = Some(None);
                i += 2;
            }
            b'[' => {
                in_class = true;
                current = Some(None);
                i += 1;
            }
            b'(' => {
                // Entering a group resets the run -- alternations we
                // detect are at the current depth only.
                if depth == 0 {
                    flush(&mut run, &mut current);
                    if alternation_hit(&run) {
                        return true;
                    }
                    run.clear();
                }
                depth += 1;
                i += 1;
            }
            b')' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        // Group closed -- the group itself counts as a
                        // (disqualified) token of the outer run.
                        current = Some(None);
                        run.clear();
                    }
                }
                i += 1;
            }
            b'|' if depth == 0 => {
                flush(&mut run, &mut current);
                i += 1;
            }
            b if b.is_ascii_alphanumeric() => {
                // First byte of a token -> single char; second byte
                // disqualifies it.
                match current {
                    None => current = Some(Some(b)),
                    Some(_) => current = Some(None),
                }
                i += 1;
            }
            _ => {
                // Quantifiers, anchors, `.`, etc. disqualify the token.
                current = Some(None);
                i += 1;
            }
        }
    }
    flush(&mut run, &mut current);
    alternation_hit(&run)
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_single_char_alternation(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-prefer-char-class",
        "Single-character alternation \u{2014} use a character class like `[abc]` instead of `a|b|c`.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_three_char_alternation() {
        assert_eq!(run_on("const re = /a|b|c/;").len(), 1);
    }

    #[test]
    fn flags_four_char_alternation() {
        assert_eq!(run_on("const re = /x|y|z|w/;").len(), 1);
    }

    #[test]
    fn allows_multi_char_alternatives() {
        assert!(run_on("const re = /foo|bar|baz/;").is_empty());
    }

    #[test]
    fn allows_two_char_alternation() {
        assert!(run_on("const re = /a|b/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_pipe_in_string() {
        // Tailwind arbitrary-value with pipe-separated tokens in a string.
        let src = r#"const x = "grid-cols-[a|b|c]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_with_pipes_in_string() {
        let src = r#"const u = "https://example.com/?q=a|b|c";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_with_pipe_like_chars() {
        // Single-char alternation look-alike inside an import specifier.
        let src = r#"import X from "@scope/a|b|c";"#;
        assert!(run_on(src).is_empty());
    }
}

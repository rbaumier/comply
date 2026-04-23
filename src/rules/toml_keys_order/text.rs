//! toml-keys-order — line-scan each TOML section and flag keys that appear
//! out of alphabetical order.
//!
//! Line scanning rather than full TOML parsing preserves source order (the
//! `toml` crate's `Map` backing is controlled by the `preserve_order` cargo
//! feature and we don't want correctness to depend on feature flags).
//!
//! Detection: for every line that starts with `key = …`, compare the key
//! with the previous sibling key in the same section. A section boundary
//! is any `[header]` / `[[header]]` line, which resets the comparison
//! cursor. Multi-line values (inline tables on one line, strings on one
//! line, arrays on one line) are handled by stopping at the first `=`
//! outside a quoted key. Multi-line arrays/strings that span several
//! physical lines are rare in hand-written TOML and would require a full
//! parser; we accept that edge case and do not scan inside them.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.path.extension().and_then(|e| e.to_str()) != Some("toml") {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let mut prev_key: Option<String> = None;
        let mut in_multiline = false;
        for (idx, raw_line) in ctx.source.lines().enumerate() {
            // Multi-line string / array handling: once we enter a triple-
            // quoted string or an unclosed `[`, ignore every line until
            // the balancing token. Simplified: track whether the previous
            // key's value line left an unclosed `"""`, `'''`, `[`, or `{`.
            if in_multiline {
                if closes_multiline(raw_line) {
                    in_multiline = false;
                }
                continue;
            }
            let line = strip_comment(raw_line);
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if is_header(trimmed) {
                // Entering a new section — reset the comparison cursor.
                prev_key = None;
                continue;
            }
            let Some(key) = parse_key(trimmed) else {
                continue;
            };
            if opens_multiline(trimmed) {
                in_multiline = true;
            }
            if let Some(prev) = &prev_key {
                if key.as_str() < prev.as_str() {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "toml-keys-order".into(),
                        message: format!(
                            "TOML key `{key}` appears after `{prev}` — keys within a \
                             table should be declared in alphabetical order."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            prev_key = Some(key);
        }
        diagnostics
    }
}

fn is_header(trimmed: &str) -> bool {
    trimmed.starts_with('[')
}

/// Parse the key from a `key = value` line. Handles bare, quoted, and
/// dotted keys. Returns None if the line isn't a key assignment.
fn parse_key(line: &str) -> Option<String> {
    let eq = find_top_level_equals(line)?;
    let key_part = line[..eq].trim();
    if key_part.is_empty() {
        return None;
    }
    Some(key_part.to_string())
}

/// Find the index of the first `=` that is NOT inside a quoted key.
fn find_top_level_equals(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_str: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match in_str {
            Some(q) if b == q => in_str = None,
            Some(_) => {}
            None => match b {
                b'"' | b'\'' => in_str = Some(b),
                b'=' => return Some(i),
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// Strip a `#` comment from a line, respecting quoted strings.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_str: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match in_str {
            Some(q) if b == q => in_str = None,
            Some(_) => {}
            None => match b {
                b'"' | b'\'' => in_str = Some(b),
                b'#' => return &line[..i],
                _ => {}
            },
        }
        i += 1;
    }
    line
}

/// Heuristic: the value side of this line leaves an unbalanced `[` or `{`
/// or a triple-quoted string open. Multi-line strings of the form `"""` on
/// the same line are only unclosed when there are an odd number of `"""`
/// occurrences. We treat any of these as "multi-line" and skip subsequent
/// lines until the balance returns to zero.
fn opens_multiline(line: &str) -> bool {
    let triple_double = line.matches("\"\"\"").count();
    let triple_single = line.matches("'''").count();
    if triple_double % 2 == 1 || triple_single % 2 == 1 {
        return true;
    }
    let (brackets, braces) = count_open_brackets(line);
    brackets > 0 || braces > 0
}

/// Returns `(unclosed_brackets, unclosed_braces)`. Ignores content inside
/// single-line quoted strings.
fn count_open_brackets(line: &str) -> (i32, i32) {
    let bytes = line.as_bytes();
    let mut brackets = 0i32;
    let mut braces = 0i32;
    let mut in_str: Option<u8> = None;
    for &b in bytes {
        match in_str {
            Some(q) if b == q => in_str = None,
            Some(_) => {}
            None => match b {
                b'"' | b'\'' => in_str = Some(b),
                b'[' => brackets += 1,
                b']' => brackets -= 1,
                b'{' => braces += 1,
                b'}' => braces -= 1,
                _ => {}
            },
        }
    }
    (brackets.max(0), braces.max(0))
}

fn closes_multiline(line: &str) -> bool {
    // Crude: any line with `"""`, `'''`, `]`, or `}` is assumed to close
    // whatever we're in. Good enough for the hand-written TOML that
    // triggers multi-line values.
    line.contains("\"\"\"")
        || line.contains("'''")
        || line.contains(']')
        || line.contains('}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.toml"), source))
    }

    #[test]
    fn flags_out_of_order_keys() {
        let src = "[section]\nzebra = 1\nalpha = 2\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_alphabetical_keys() {
        let src = "[section]\nalpha = 1\nzebra = 2\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn order_resets_at_section_boundary() {
        // Second section's `a` comes after first section's `z` but that's
        // a different scope, so no diagnostic.
        let src = "[one]\na = 1\nz = 2\n\n[two]\na = 3\nz = 4\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn handles_quoted_keys() {
        let src = "[section]\n\"zeta\" = 1\n\"alpha\" = 2\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_comments() {
        let src = "[section]\n# alpha\nzebra = 1\n# zzz\nyak = 2\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_toml_files() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("t.ts"),
            "zebra = 1\nalpha = 2\n",
        ));
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_multiline_array_values() {
        // Entries within the multi-line `deps` value must NOT be compared
        // as keys of `[pkg]`.
        let src = "[pkg]\nname = \"x\"\ndeps = [\n  \"z\",\n  \"a\",\n]\n";
        assert!(run(src).is_empty());
    }
}

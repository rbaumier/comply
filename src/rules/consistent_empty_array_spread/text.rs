use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects `...expr ? ` pattern (spread of an unparenthesized ternary).
/// Matches `...identifier ?` or `...complex.expr ?` but NOT `...(expr ? `.
fn has_unparenthesized_spread_ternary(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("...") {
        let abs = start + pos + 3; // skip past "..."
        let rest = &line[abs..];

        // If immediately followed by `(`, the ternary is parenthesized — OK.
        let trimmed = rest.trim_start();
        if trimmed.starts_with('(') {
            start = abs;
            continue;
        }

        // Look for a `?` that's part of a ternary (not `?.` optional chaining).
        // Scan forward for `?` in the rest of the line.
        let mut i = 0;
        let bytes = trimmed.as_bytes();
        let mut depth_paren = 0i32;
        let mut depth_bracket = 0i32;
        let mut found = false;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth_paren += 1,
                b')' => {
                    depth_paren -= 1;
                    if depth_paren < 0 {
                        break;
                    }
                }
                b'[' => depth_bracket += 1,
                b']' => {
                    depth_bracket -= 1;
                    if depth_bracket < 0 {
                        break;
                    }
                }
                b'?' if depth_paren == 0 && depth_bracket == 0 => {
                    // Make sure it's not `?.` (optional chaining)
                    if i + 1 < bytes.len() && bytes[i + 1] == b'.' {
                        i += 2;
                        continue;
                    }
                    // Make sure it's not `??` (nullish coalescing)
                    if i + 1 < bytes.len() && bytes[i + 1] == b'?' {
                        i += 2;
                        continue;
                    }
                    found = true;
                    break;
                }
                b'\'' | b'"' | b'`' => {
                    // Skip string content
                    let quote = bytes[i];
                    i += 1;
                    while i < bytes.len() && bytes[i] != quote {
                        if bytes[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }

        if found {
            return true;
        }

        start = abs;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_unparenthesized_spread_ternary(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "consistent-empty-array-spread".into(),
                    message: "Parenthesize the ternary in array spread: \
                              `[...(condition ? ['a'] : [])]`."
                        .into(),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_unparenthesized_ternary_spread() {
        assert_eq!(
            run("const arr = [...condition ? ['a'] : []];").len(),
            1
        );
    }

    #[test]
    fn allows_parenthesized_ternary_spread() {
        assert!(run("const arr = [...(condition ? ['a'] : [])];").is_empty());
    }

    #[test]
    fn flags_complex_condition() {
        assert_eq!(
            run("const arr = [...a && b ? [1] : []];").len(),
            1
        );
    }

    #[test]
    fn allows_normal_spread() {
        assert!(run("const arr = [...items];").is_empty());
    }

    #[test]
    fn allows_optional_chaining_spread() {
        assert!(run("const arr = [...obj?.items];").is_empty());
    }
}

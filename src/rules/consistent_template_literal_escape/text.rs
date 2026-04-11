use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects `$\{` or `\$\{` inside template literals (backtick strings).
///
/// The correct escape is `\${` — escaping the dollar sign only.
/// Escaping the brace (`$\{`) or both (`\$\{`) is inconsistent.
impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }

            if has_bad_template_escape(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "consistent-template-literal-escape".into(),
                    message: "Use `\\${` instead of `$\\{` to escape in template literals.".into(),
                    severity: Severity::Warning,
                });
            }
        }

        diagnostics
    }
}

/// Check if a line contains `$\{` or `\$\{` inside a template literal.
///
/// Strategy: walk the line tracking whether we're inside a backtick string.
/// Inside a template literal, look for:
///   - `$\{` (dollar then backslash-brace) — bad
///   - `\$\{` (backslash-dollar then backslash-brace) — bad
///
/// We must NOT flag `\${` (backslash-dollar-brace) which is the correct escape.
fn has_bad_template_escape(line: &str) -> bool {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut in_template = false;
    let mut in_single = false;
    let mut in_double = false;
    let mut i = 0;

    while i < len {
        let b = bytes[i];

        // Inside regular strings, skip escape sequences
        if (in_single || in_double) && b == b'\\' && i + 1 < len {
            i += 2;
            continue;
        }

        // Track single-quote strings
        if !in_template && !in_double && b == b'\'' {
            in_single = !in_single;
            i += 1;
            continue;
        }

        // Track double-quote strings
        if !in_template && !in_single && b == b'"' {
            in_double = !in_double;
            i += 1;
            continue;
        }

        // Skip if we're inside a regular string
        if in_single || in_double {
            i += 1;
            continue;
        }

        // Track template literals
        if b == b'`' {
            in_template = !in_template;
            i += 1;
            continue;
        }

        if in_template {
            // Skip normal template expressions `${...}`
            if b == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
                // This is a real interpolation, skip to closing brace
                i += 2;
                let mut depth = 1i32;
                while i < len && depth > 0 {
                    if bytes[i] == b'{' {
                        depth += 1;
                    } else if bytes[i] == b'}' {
                        depth -= 1;
                    } else if bytes[i] == b'\\' && i + 1 < len {
                        i += 1; // skip escaped char inside expression
                    }
                    i += 1;
                }
                continue;
            }

            // Check for `\$\{` — backslash-dollar-backslash-brace (bad: escapes both)
            if b == b'\\'
                && i + 3 < len
                && bytes[i + 1] == b'$'
                && bytes[i + 2] == b'\\'
                && bytes[i + 3] == b'{'
            {
                // Make sure the leading backslash is not itself escaped
                if !is_preceded_by_odd_backslashes(bytes, i) {
                    return true;
                }
            }

            // Check for `$\{` — dollar-backslash-brace (bad: escapes only the brace)
            if b == b'$' && i + 2 < len && bytes[i + 1] == b'\\' && bytes[i + 2] == b'{' {
                // Make sure the `$` is not itself escaped
                if !is_preceded_by_odd_backslashes(bytes, i) {
                    return true;
                }
            }

            // Handle `\${` — this is the CORRECT pattern, skip past it
            if b == b'\\' && i + 2 < len && bytes[i + 1] == b'$' && bytes[i + 2] == b'{' {
                // Only if the backslash is not itself escaped
                if !is_preceded_by_odd_backslashes(bytes, i) {
                    i += 3; // skip `\${`
                    continue;
                }
            }

            // Skip other escape sequences in template
            if b == b'\\' && i + 1 < len {
                i += 2;
                continue;
            }
        }

        i += 1;
    }

    false
}

/// Check if position `pos` is preceded by an odd number of backslashes.
fn is_preceded_by_odd_backslashes(bytes: &[u8], pos: usize) -> bool {
    let mut count = 0;
    let mut p = pos;
    while p > 0 && bytes[p - 1] == b'\\' {
        count += 1;
        p -= 1;
    }
    count % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    // --- Bad patterns (should flag) ---

    #[test]
    fn flags_dollar_backslash_brace() {
        // Source: `$\{foo}` — escaping the brace instead of the dollar
        let d = run(r#"const s = `$\{foo}`;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_escaped_dollar_and_brace() {
        // Source: `\$\{foo}` — escaping both dollar and brace
        let d = run(r#"const s = `\$\{foo}`;"#);
        assert_eq!(d.len(), 1);
    }

    // --- Good patterns (should not flag) ---

    #[test]
    fn allows_backslash_dollar_brace() {
        // Source: `\${foo}` — correct: escaping only the dollar
        let d = run(r#"const s = `\${foo}`;"#);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_normal_interpolation() {
        let d = run(r#"const s = `${foo}`;"#);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_regular_string() {
        // In a regular string, not a template literal — not our concern
        let d = run(r#"const s = "$\{foo}";"#);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_comment() {
        let d = run(r#"// `$\{foo}`"#);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_plain_template() {
        let d = run(r#"const s = `hello world`;"#);
        assert!(d.is_empty());
    }
}

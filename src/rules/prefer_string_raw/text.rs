use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Count occurrences of `\\` (escaped backslashes) in a string literal's raw source.
/// We look for `\\` sequences inside quotes. A string with 2+ escaped backslashes
/// is a candidate for `String.raw`.
fn count_escaped_backslashes(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'\\' {
            count += 1;
            i += 2; // skip the pair
        } else {
            i += 1;
        }
    }
    count
}

/// Check if a line contains a string literal (single or double quoted) with
/// multiple escaped backslashes. We skip lines that contain backticks (template
/// literals) or `${` (interpolation) since those can't use String.raw simply,
/// and skip lines that already use `String.raw`.
fn has_multiple_escaped_backslashes(line: &str) -> bool {
    // Already using String.raw — nothing to flag
    if line.contains("String.raw") {
        return false;
    }

    // Skip lines with backtick chars inside string content (can't use String.raw with backticks)
    // We check for backtick presence as a simple heuristic
    if line.contains('`') {
        return false;
    }

    // Find string literals in single or double quotes and check them
    for quote in [b'\'', b'"'] {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == quote {
                // Find matching close quote, respecting escapes
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escaped char
                        continue;
                    }
                    if bytes[i] == quote {
                        // Found the string literal from start..=i
                        let literal = &line[start..=i];
                        if literal.contains("${") {
                            i += 1;
                            break;
                        }
                        if count_escaped_backslashes(literal) >= 2 {
                            return true;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_multiple_escaped_backslashes(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-string-raw".into(),
                    message:
                        "`String.raw` should be used to avoid escaping `\\`.".into(),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }

    #[test]
    fn flags_multiple_escaped_backslashes_double_quotes() {
        let d = run(r#"const p = "C:\\Users\\foo\\bar";"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_multiple_escaped_backslashes_single_quotes() {
        let d = run(r#"const p = 'C:\\Users\\foo';"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_single_escaped_backslash() {
        assert!(run(r#"const p = "foo\\bar";"#).is_empty());
    }

    #[test]
    fn allows_no_backslash() {
        assert!(run(r#"const p = "hello world";"#).is_empty());
    }

    #[test]
    fn allows_string_raw_already() {
        assert!(run(r#"const p = String.raw`C:\Users\foo\bar`;"#).is_empty());
    }

    #[test]
    fn allows_template_with_backtick() {
        // Line contains backtick — skip it
        assert!(run("const p = `foo\\\\bar\\\\baz`;").is_empty());
    }
}

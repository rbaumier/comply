use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect patterns like `!identifier ===` or `!identifier !==`.
fn has_inverted_check(line: &str) -> bool {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'!' {
            // Make sure this is not `!==` (a comparison operator).
            if i + 1 < len && bytes[i + 1] == b'=' {
                i += 1;
                continue;
            }
            // Skip optional whitespace after `!`.
            let mut j = i + 1;
            while j < len && bytes[j] == b' ' {
                j += 1;
            }
            // Expect an identifier char (letter, _, $).
            if j < len
                && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_' || bytes[j] == b'$')
            {
                // Skip identifier.
                let mut k = j;
                while k < len
                    && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_' || bytes[k] == b'$' || bytes[k] == b'.')
                {
                    k += 1;
                }
                // Skip whitespace.
                while k < len && bytes[k] == b' ' {
                    k += 1;
                }
                // Check for `===` or `!==`.
                if k + 2 < len {
                    if (bytes[k] == b'=' && bytes[k + 1] == b'=' && bytes[k + 2] == b'=')
                        || (bytes[k] == b'!' && bytes[k + 1] == b'=' && bytes[k + 2] == b'=')
                    {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_inverted_check(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-inverted-boolean-check".into(),
                    message: "`!a === b` negates `a` before comparing — use `a !== b` or `!(a === b)`.".into(),
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
    fn flags_not_a_strict_equals_b() {
        assert_eq!(run("if (!a === b) {}").len(), 1);
    }

    #[test]
    fn flags_not_a_strict_not_equals_b() {
        assert_eq!(run("if (!a !== b) {}").len(), 1);
    }

    #[test]
    fn flags_with_member_access() {
        assert_eq!(run("if (!foo.bar === baz) {}").len(), 1);
    }

    #[test]
    fn allows_normal_comparison() {
        assert!(run("if (a === b) {}").is_empty());
    }

    #[test]
    fn allows_negated_result() {
        assert!(run("if (!(a === b)) {}").is_empty());
    }

    #[test]
    fn allows_not_equals_operator() {
        assert!(run("if (a !== b) {}").is_empty());
    }
}

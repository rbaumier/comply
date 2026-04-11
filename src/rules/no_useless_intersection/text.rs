use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `& any`, `& unknown`, `any &`, `unknown &` in type expressions.
fn has_useless_intersection(line: &str) -> bool {
    let trimmed = line.trim();
    // Check for `& any` or `& unknown` (with word boundaries via surrounding context)
    for pattern in &["& any", "& unknown", "any &", "unknown &"] {
        if let Some(pos) = trimmed.find(pattern) {
            let before = if pos > 0 {
                trimmed.as_bytes()[pos - 1]
            } else {
                b' '
            };
            let end = pos + pattern.len();
            let after = if end < trimmed.len() {
                trimmed.as_bytes()[end]
            } else {
                b' '
            };
            // Ensure surrounding chars are not alphanumeric (word boundary)
            if !before.is_ascii_alphanumeric() && before != b'_'
                && !after.is_ascii_alphanumeric() && after != b'_'
            {
                return true;
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_useless_intersection(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-useless-intersection".into(),
                    message: "Intersection with `any` or `unknown` is useless — remove it.".into(),
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
    fn flags_intersection_with_any() {
        assert_eq!(run("type X = Foo & any;").len(), 1);
    }

    #[test]
    fn flags_intersection_with_unknown() {
        assert_eq!(run("type X = Foo & unknown;").len(), 1);
    }

    #[test]
    fn flags_any_on_left() {
        assert_eq!(run("type X = any & Foo;").len(), 1);
    }

    #[test]
    fn allows_normal_intersection() {
        assert!(run("type X = Foo & Bar;").is_empty());
    }

    #[test]
    fn no_false_positive_on_any_prefix() {
        // `anything` contains `any` but should not match
        assert!(run("type X = anything & Foo;").is_empty());
    }
}

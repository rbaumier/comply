use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Patterns that indicate a `typeof x === 'undefined'` comparison.
/// We check for both quote styles and all four equality operators.
const UNDEFINED_STRINGS: &[&str] = &["'undefined'", "\"undefined\""];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Must contain `typeof` and one of the undefined string literals.
            if !line.contains("typeof ") {
                continue;
            }
            for pat in UNDEFINED_STRINGS {
                if line.contains(pat) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-typeof-undefined".into(),
                        message: "Compare with `undefined` directly instead of using `typeof`. \
                                  Replace `typeof x === 'undefined'` with `x === undefined`."
                            .into(),
                        severity: Severity::Warning,
                    });
                    break; // one diagnostic per line
                }
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
    fn flags_typeof_triple_equals_single_quotes() {
        assert_eq!(run("if (typeof x === 'undefined') {}").len(), 1);
    }

    #[test]
    fn flags_typeof_triple_equals_double_quotes() {
        assert_eq!(run(r#"if (typeof x === "undefined") {}"#).len(), 1);
    }

    #[test]
    fn flags_typeof_double_equals() {
        assert_eq!(run("if (typeof x == 'undefined') {}").len(), 1);
    }

    #[test]
    fn flags_typeof_not_equals() {
        assert_eq!(run("if (typeof x !== 'undefined') {}").len(), 1);
    }

    #[test]
    fn allows_direct_undefined_comparison() {
        assert!(run("if (x === undefined) {}").is_empty());
    }

    #[test]
    fn allows_typeof_for_other_types() {
        assert!(run("if (typeof x === 'string') {}").is_empty());
    }

    #[test]
    fn allows_undefined_in_comment() {
        // No typeof keyword on this line.
        assert!(run("// check for 'undefined'").is_empty());
    }
}

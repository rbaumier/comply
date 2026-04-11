use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const WRAPPER_PATTERNS: &[&str] = &["new String(", "new Number(", "new Boolean("];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for pattern in WRAPPER_PATTERNS {
                if line.contains(pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-primitive-wrappers".into(),
                        message: format!(
                            "Primitive wrapper object detected — `{}...)` creates an object, not a primitive. Use `{}...)` without `new`.",
                            pattern,
                            &pattern[4..] // strip "new "
                        ),
                        severity: Severity::Error,
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
    fn flags_new_string() {
        assert_eq!(run(r#"const s = new String("hello");"#).len(), 1);
    }

    #[test]
    fn flags_new_number() {
        assert_eq!(run("const n = new Number(42);").len(), 1);
    }

    #[test]
    fn flags_new_boolean() {
        assert_eq!(run("const b = new Boolean(true);").len(), 1);
    }

    #[test]
    fn allows_factory_calls() {
        assert!(run(r#"const s = String("hello");"#).is_empty());
        assert!(run("const n = Number(42);").is_empty());
        assert!(run("const b = Boolean(0);").is_empty());
    }

    #[test]
    fn allows_unrelated_new() {
        assert!(run("const m = new Map();").is_empty());
    }
}

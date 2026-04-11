use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PATTERNS: &[&str] = &["arguments[", "arguments.length", "arguments.callee"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for pat in PATTERNS {
                if line.contains(pat) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-arguments-usage".into(),
                        message: format!(
                            "Avoid direct use of `arguments` — use rest parameters (`...args`) instead."
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
    fn flags_arguments_bracket() {
        assert_eq!(run("const first = arguments[0];").len(), 1);
    }

    #[test]
    fn flags_arguments_length() {
        assert_eq!(run("if (arguments.length > 0) {}").len(), 1);
    }

    #[test]
    fn flags_arguments_callee() {
        assert_eq!(run("return arguments.callee;").len(), 1);
    }

    #[test]
    fn allows_rest_params() {
        assert!(run("function foo(...args) { return args[0]; }").is_empty());
    }

    #[test]
    fn one_diagnostic_per_line() {
        assert_eq!(run("arguments[0] + arguments.length").len(), 1);
    }
}

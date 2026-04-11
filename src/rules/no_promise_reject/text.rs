use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if trimmed.contains("Promise.reject(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-promise-reject".into(),
                    message: "`Promise.reject()` — prefer returning error values or throwing typed errors.".into(),
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
    fn flags_promise_reject() {
        assert_eq!(run("return Promise.reject(new Error('fail'));").len(), 1);
    }

    #[test]
    fn flags_promise_reject_no_arg() {
        assert_eq!(run("return Promise.reject();").len(), 1);
    }

    #[test]
    fn allows_promise_resolve() {
        assert!(run("return Promise.resolve(value);").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// Promise.reject() is bad").is_empty());
    }
}

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            let flagged =
                (t.contains("exec(`") && t.contains("${")) || t.contains("shell: true");
            if flagged {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-shell-exec".into(),
                    message: "Shell interpolation in `exec()` or `shell: true` allows command injection — use `execFile()` with an args array.".into(),
                    severity: Severity::Error,
                    span: None,
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
    fn flags_exec_with_template_literal() {
        assert_eq!(run("exec(`git ${cmd}`)").len(), 1);
    }
    #[test]
    fn flags_shell_true() {
        assert_eq!(run("spawn('sh', ['-c', cmd], { shell: true })").len(), 1);
    }
    #[test]
    fn allows_execfile() {
        assert!(run("execFile('git', ['status'])").is_empty());
    }
    #[test]
    fn allows_exec_literal() {
        assert!(run("exec('git status')").is_empty());
    }
}

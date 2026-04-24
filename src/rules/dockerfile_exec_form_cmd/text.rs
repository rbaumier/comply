//! dockerfile-exec-form-cmd text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            let rest = if let Some(r) = instruction_body(trimmed, "CMD") {
                r
            } else if let Some(r) = instruction_body(trimmed, "ENTRYPOINT") {
                r
            } else {
                continue;
            };
            let arg = rest.trim_start();
            if arg.is_empty() {
                continue;
            }
            if !arg.starts_with('[') {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "CMD/ENTRYPOINT must use exec form (JSON array); shell form breaks signal forwarding.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

fn instruction_body<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let kb = keyword.as_bytes();
    if line.len() <= kb.len() {
        return None;
    }
    if !line.as_bytes()[..kb.len()].eq_ignore_ascii_case(kb) {
        return None;
    }
    if !line.as_bytes()[kb.len()].is_ascii_whitespace() {
        return None;
    }
    Some(&line[kb.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_shell_form_cmd() {
        assert_eq!(run("CMD node server.js").len(), 1);
    }

    #[test]
    fn flags_shell_form_entrypoint() {
        assert_eq!(run("ENTRYPOINT /entrypoint.sh").len(), 1);
    }

    #[test]
    fn allows_exec_form() {
        assert!(run("CMD [\"node\", \"server.js\"]").is_empty());
    }
}

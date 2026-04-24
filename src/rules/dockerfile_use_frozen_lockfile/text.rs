//! dockerfile-use-frozen-lockfile text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if !is_run_line(trimmed) {
                continue;
            }
            let body = &trimmed[3..];
            let is_pnpm = body.contains("pnpm install");
            let is_yarn = body.contains("yarn install") || body.contains("yarn add");
            if !(is_pnpm || is_yarn) {
                continue;
            }
            if body.contains("--frozen-lockfile") || body.contains("--immutable") {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "pnpm/yarn install must pass `--frozen-lockfile` (or `--immutable`) in Dockerfiles.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn is_run_line(line: &str) -> bool {
    line.len() >= 4
        && line.as_bytes()[..3].eq_ignore_ascii_case(b"RUN")
        && line.as_bytes()[3].is_ascii_whitespace()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_pnpm_without_frozen() {
        assert_eq!(run("RUN pnpm install").len(), 1);
    }

    #[test]
    fn allows_pnpm_with_frozen() {
        assert!(run("RUN pnpm install --frozen-lockfile").is_empty());
    }

    #[test]
    fn allows_yarn_immutable() {
        assert!(run("RUN yarn install --immutable").is_empty());
    }
}

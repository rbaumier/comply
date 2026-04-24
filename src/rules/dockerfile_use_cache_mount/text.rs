//! dockerfile-use-cache-mount text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PACKAGE_MANAGERS: &[&str] = &[
    "npm ci",
    "npm install",
    "pnpm install",
    "yarn install",
    "pip install",
    "apt install",
    "apt-get install",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if !is_run_line(trimmed) {
                continue;
            }
            let body = &trimmed[3..];
            if !PACKAGE_MANAGERS.iter().any(|m| body.contains(m)) {
                continue;
            }
            if body.contains("--mount=type=cache") {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Package-manager RUN step should use `--mount=type=cache` for faster rebuilds.".into(),
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
    fn flags_npm_ci_without_cache() {
        assert_eq!(run("RUN npm ci").len(), 1);
    }

    #[test]
    fn allows_with_cache_mount() {
        let src = "RUN --mount=type=cache,target=/root/.npm npm ci";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_run() {
        assert!(run("RUN echo hi").is_empty());
    }
}

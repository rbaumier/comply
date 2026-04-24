//! dockerfile-require-healthcheck text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut saw_from = false;
        let mut saw_healthcheck = false;
        for line in ctx.source.lines() {
            let trimmed = line.trim_start();
            if is_keyword(trimmed, "FROM") {
                saw_from = true;
            }
            if is_keyword(trimmed, "HEALTHCHECK") {
                saw_healthcheck = true;
            }
        }
        if saw_from && !saw_healthcheck {
            vec![Diagnostic {
                path: ctx.path.to_path_buf(),
                line: 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Dockerfile missing HEALTHCHECK — orchestrators can't detect stuck containers.".into(),
                severity: Severity::Warning,
                span: None,
            }]
        } else {
            Vec::new()
        }
    }
}

fn is_keyword(line: &str, keyword: &str) -> bool {
    let kb = keyword.as_bytes();
    line.len() > kb.len()
        && line.as_bytes()[..kb.len()].eq_ignore_ascii_case(kb)
        && line.as_bytes()[kb.len()].is_ascii_whitespace()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_missing_healthcheck() {
        assert_eq!(run("FROM node:22.12\nCMD [\"node\"]\n").len(), 1);
    }

    #[test]
    fn allows_healthcheck_present() {
        let src = "FROM node:22.12\nHEALTHCHECK CMD curl -f http://localhost/ || exit 1\nCMD [\"node\"]\n";
        assert!(run(src).is_empty());
    }
}

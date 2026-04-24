//! dockerfile-pin-exact-version text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if !is_from_line(trimmed) {
                continue;
            }
            let rest = trimmed[4..].trim_start();
            let tokens: Vec<&str> = rest
                .split_whitespace()
                .filter(|t| !t.starts_with("--"))
                .collect();
            let Some(image) = tokens.first() else { continue };
            if *image == "scratch" || image.contains('@') {
                continue;
            }
            let Some((_, tag)) = image.split_once(':') else {
                continue; // no tag at all — handled by dockerfile-no-latest-tag
            };
            if tag == "latest" {
                continue; // handled by dockerfile-no-latest-tag
            }
            if is_bare_major(tag) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "FROM tag pins only a major version; pin a precise version such as `22.12-alpine3.20`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

fn is_from_line(line: &str) -> bool {
    line.len() >= 5
        && line.as_bytes()[..4].eq_ignore_ascii_case(b"FROM")
        && line.as_bytes()[4].is_ascii_whitespace()
}

/// A tag is "bare major" when it contains only digits, e.g. `22` or `3`.
fn is_bare_major(tag: &str) -> bool {
    !tag.is_empty() && tag.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_bare_major_tag() {
        assert_eq!(run("FROM node:22").len(), 1);
    }

    #[test]
    fn allows_precise_tag() {
        assert!(run("FROM node:22.12-alpine3.20").is_empty());
    }

    #[test]
    fn ignores_latest() {
        // Handled by sibling rule.
        assert!(run("FROM node:latest").is_empty());
    }
}

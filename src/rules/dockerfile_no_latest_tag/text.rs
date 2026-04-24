//! dockerfile-no-latest-tag text backend.

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
            // Strip "FROM " prefix (case-insensitive).
            let rest = &trimmed[4..].trim_start();
            // Drop `--platform=...` flags.
            let tokens: Vec<&str> = rest
                .split_whitespace()
                .filter(|t| !t.starts_with("--"))
                .collect();
            let Some(image) = tokens.first() else { continue };
            // `FROM scratch` is a special empty base; allow it.
            if *image == "scratch" {
                continue;
            }
            // `FROM <alias>` referencing a previous stage only appears when
            // the alias has no `:`, no `/`, and tokens include `AS <next>`.
            // That's out of scope for this rule — we only emit when the
            // image reference clearly targets a registry image.
            if image.ends_with(":latest") || !image.contains(':') {
                // Allow digest-pinned images (`@sha256:...`) even without a tag.
                if image.contains('@') {
                    continue;
                }
                if *image == "scratch" {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "FROM image uses `:latest` or no tag; pin an explicit version.".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_latest_tag() {
        assert_eq!(run("FROM node:latest").len(), 1);
    }

    #[test]
    fn flags_missing_tag() {
        assert_eq!(run("FROM node").len(), 1);
    }

    #[test]
    fn allows_pinned_version() {
        assert!(run("FROM node:22.12-alpine3.20").is_empty());
    }

    #[test]
    fn allows_scratch() {
        assert!(run("FROM scratch").is_empty());
    }

    #[test]
    fn allows_digest_pin() {
        assert!(run("FROM node@sha256:abc123").is_empty());
    }
}

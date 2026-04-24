//! dockerfile-copy-after-install text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // Track `COPY . .` occurrences per stage, and whether an install has
        // been seen in the current stage.
        let mut install_seen = false;
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if is_from_line(trimmed) {
                install_seen = false;
                continue;
            }
            if is_install_run(trimmed) {
                install_seen = true;
                continue;
            }
            if is_copy_all(trimmed) && !install_seen {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`COPY . .` before dependency install — copy the lockfile, run install, then copy the rest to keep layer caching effective.".into(),
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

fn is_install_run(line: &str) -> bool {
    if !(line.len() >= 4
        && line.as_bytes()[..3].eq_ignore_ascii_case(b"RUN")
        && line.as_bytes()[3].is_ascii_whitespace())
    {
        return false;
    }
    let body = &line[3..];
    (body.contains("npm install") || body.contains("npm ci"))
        || (body.contains("yarn install") || body.contains("yarn ")) && body.contains("install")
        || body.contains("pnpm install")
        || body.contains("pip install")
}

/// Match `COPY . .` or `COPY . /app` (any single-dot source).
fn is_copy_all(line: &str) -> bool {
    if !(line.len() >= 5
        && line.as_bytes()[..4].eq_ignore_ascii_case(b"COPY")
        && line.as_bytes()[4].is_ascii_whitespace())
    {
        return false;
    }
    let rest = line[4..].trim_start();
    // Ignore --from/--chown flags.
    let tokens: Vec<&str> = rest
        .split_whitespace()
        .filter(|t| !t.starts_with("--"))
        .collect();
    tokens.len() >= 2 && tokens[0] == "."
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_copy_before_install() {
        let src = "FROM node:22.12\nCOPY . .\nRUN npm ci\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_copy_after_install() {
        let src = "FROM node:22.12\nCOPY package.json package-lock.json ./\nRUN npm ci\nCOPY . .\n";
        assert!(run(src).is_empty());
    }
}

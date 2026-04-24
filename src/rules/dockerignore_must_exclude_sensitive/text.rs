//! dockerignore-must-exclude-sensitive text backend.
//!
//! Filesystem-level `.dockerignore` reads aren't wired yet; this rule flags
//! the Dockerfile side so users are reminded to curate `.dockerignore`
//! whenever they ship a broad `COPY .`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if is_copy_all(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`COPY .` requires `.dockerignore` to exclude `.env*`, `.git`, `node_modules`, `*.pem`, `id_rsa`, `.npmrc`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                break; // One reminder per file is enough.
            }
        }
        diagnostics
    }
}

fn is_copy_all(line: &str) -> bool {
    if !(line.len() >= 5
        && line.as_bytes()[..4].eq_ignore_ascii_case(b"COPY")
        && line.as_bytes()[4].is_ascii_whitespace())
    {
        return false;
    }
    let rest = line[4..].trim_start();
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
    fn flags_copy_all() {
        assert_eq!(run("COPY . .").len(), 1);
    }

    #[test]
    fn allows_specific_copy() {
        assert!(run("COPY package.json ./").is_empty());
    }
}

//! dockerfile-require-dockerignore text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !is_copy_all(trimmed) {
                continue;
            }
            // If the immediately preceding comment line mentions
            // "dockerignore", consider it acknowledged.
            let acknowledged = idx > 0 && lines[idx - 1].trim_start().starts_with('#')
                && lines[idx - 1].to_ascii_lowercase().contains("dockerignore");
            if acknowledged {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Broad `COPY .` without `.dockerignore` acknowledgement — make sure `node_modules`, `.git`, `.env` are excluded.".into(),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_copy_all_without_comment() {
        assert_eq!(run("COPY . .").len(), 1);
    }

    #[test]
    fn allows_copy_all_with_dockerignore_comment() {
        let src = "# .dockerignore excludes node_modules and .env\nCOPY . .\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_specific_copy() {
        assert!(run("COPY package.json ./").is_empty());
    }
}

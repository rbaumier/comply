//! dockerfile-require-multi-stage text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut from_lines: Vec<(usize, String)> = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if is_from_line(trimmed) {
                from_lines.push((idx + 1, trimmed.to_string()));
            }
        }
        // Multi-stage if there are 2+ FROM lines OR any FROM uses AS.
        let multi = from_lines.len() >= 2
            || from_lines
                .iter()
                .any(|(_, l)| contains_as_keyword(l));
        if multi || from_lines.is_empty() {
            return Vec::new();
        }
        let (line_no, _) = &from_lines[0];
        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: *line_no,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Single-stage Dockerfile — use `FROM ... AS build` plus a runtime stage to keep the final image minimal.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

fn is_from_line(line: &str) -> bool {
    line.len() >= 5
        && line.as_bytes()[..4].eq_ignore_ascii_case(b"FROM")
        && line.as_bytes()[4].is_ascii_whitespace()
}

fn contains_as_keyword(line: &str) -> bool {
    // Case-insensitive search for ` AS ` as a whole word.
    let lower = line.to_ascii_lowercase();
    lower.split_whitespace().any(|t| t == "as")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_single_stage() {
        assert_eq!(run("FROM node:22.12\nRUN npm ci\n").len(), 1);
    }

    #[test]
    fn allows_explicit_as() {
        assert!(run("FROM node:22.12 AS build\nRUN npm ci\n").is_empty());
    }

    #[test]
    fn allows_two_stages() {
        let src = "FROM node:22.12 AS build\nRUN npm ci\nFROM nginx:1.27.3\n";
        assert!(run(src).is_empty());
    }
}

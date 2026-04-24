//! dockerfile-require-non-root-user text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut last_user: Option<String> = None;
        let mut saw_from = false;
        for line in ctx.source.lines() {
            let trimmed = line.trim_start();
            if is_from_line(trimmed) {
                saw_from = true;
                // New stage resets the active user.
                last_user = None;
                continue;
            }
            if let Some(arg) = instruction_body(trimmed, "USER") {
                last_user = Some(arg.trim().to_string());
            }
        }
        if !saw_from {
            return Vec::new();
        }
        let flagged = match last_user.as_deref() {
            None => true,
            Some(u) => u == "root" || u == "0" || u.starts_with("root:") || u.starts_with("0:"),
        };
        if flagged {
            vec![Diagnostic {
                path: ctx.path.to_path_buf(),
                line: 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Dockerfile must drop to a non-root USER before CMD.".into(),
                severity: Severity::Warning,
                span: None,
            }]
        } else {
            Vec::new()
        }
    }
}

fn is_from_line(line: &str) -> bool {
    line.len() >= 5
        && line.as_bytes()[..4].eq_ignore_ascii_case(b"FROM")
        && line.as_bytes()[4].is_ascii_whitespace()
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
    fn flags_missing_user() {
        let src = "FROM node:22.12\nCMD [\"node\", \"a.js\"]\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_user_root() {
        let src = "FROM node:22.12\nUSER root\nCMD [\"node\"]\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_root_user() {
        let src = "FROM node:22.12\nUSER node\nCMD [\"node\"]\n";
        assert!(run(src).is_empty());
    }
}

//! dockerfile-no-secrets-in-arg text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const SECRET_SUBSTRINGS: &[&str] = &["SECRET", "TOKEN", "PASSWORD", "PASSWD", "APIKEY"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            let Some(body) = instruction_body(trimmed, "ARG") else {
                continue;
            };
            let body = body.trim();
            // ARG allows only one variable per line; pick the first token.
            let Some(token) = body.split_whitespace().next() else {
                continue;
            };
            let Some((key, value)) = token.split_once('=') else {
                continue; // No default value — fine.
            };
            if !is_secret_name(key) || value.is_empty() {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "ARG `{key}` has a secret-like default; use `--mount=type=secret` instead."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
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

fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SECRET_SUBSTRINGS.iter().any(|m| upper.contains(m)) || upper.ends_with("_KEY")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_arg_with_secret_default() {
        assert_eq!(run("ARG NPM_TOKEN=abcdef").len(), 1);
    }

    #[test]
    fn allows_arg_without_default() {
        assert!(run("ARG NPM_TOKEN").is_empty());
    }

    #[test]
    fn allows_non_secret_arg() {
        assert!(run("ARG NODE_VERSION=22.12").is_empty());
    }
}

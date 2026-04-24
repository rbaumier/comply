//! dockerfile-no-secrets-in-copy text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            let Some(body) = instruction_body(trimmed, "COPY") else {
                continue;
            };
            // Drop flags like --from=..., --chown=..., --chmod=...
            let tokens: Vec<&str> = body
                .split_whitespace()
                .filter(|t| !t.starts_with("--"))
                .collect();
            if tokens.len() < 2 {
                continue;
            }
            // Last token is destination — check only sources.
            let sources = &tokens[..tokens.len() - 1];
            for src in sources {
                if let Some(reason) = matches_secret(src) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "COPY source `{src}` looks like a credential file ({reason}); add it to `.dockerignore`."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
            }
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

fn matches_secret(src: &str) -> Option<&'static str> {
    let s = src.trim_matches('"').trim_matches('\'');
    let basename = s.rsplit('/').next().unwrap_or(s);
    if basename == ".env" || basename.starts_with(".env.") {
        return Some(".env file");
    }
    if basename.ends_with(".pem") || basename.ends_with(".key") {
        return Some("private key");
    }
    if basename == "id_rsa" || basename == "id_dsa" || basename == "id_ecdsa" || basename == "id_ed25519" {
        return Some("SSH private key");
    }
    if basename == ".npmrc" || basename == ".yarnrc" || basename == ".netrc" {
        return Some("package manager credential file");
    }
    if basename == ".aws" || s.contains("/.aws/") {
        return Some("AWS credentials");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_copy_env_file() {
        assert_eq!(run("COPY .env /app/.env").len(), 1);
    }

    #[test]
    fn flags_copy_pem() {
        assert_eq!(run("COPY server.pem /etc/ssl/server.pem").len(), 1);
    }

    #[test]
    fn flags_copy_npmrc() {
        assert_eq!(run("COPY .npmrc /root/.npmrc").len(), 1);
    }

    #[test]
    fn allows_regular_copy() {
        assert!(run("COPY package.json ./").is_empty());
    }
}

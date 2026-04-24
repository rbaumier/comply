//! dockerfile-no-secrets-in-env text backend.

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
            let Some(body) = instruction_body(trimmed, "ENV") else {
                continue;
            };
            for (key, value) in parse_env_pairs(body) {
                if !has_secret_marker(&key) {
                    continue;
                }
                if value.is_empty() {
                    continue;
                }
                // Accept build-arg passthroughs: `ENV FOO=${FOO}` or `$FOO`.
                if is_pure_var_ref(&value) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "ENV `{key}` embeds a secret-like literal in the image layer."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                break;
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

fn has_secret_marker(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    // `KEY` alone is too noisy (e.g. `ENV PRIMARY_KEY_TYPE=...`); require the
    // longer markers to reduce false positives.
    SECRET_SUBSTRINGS.iter().any(|m| upper.contains(m))
        || upper.ends_with("_KEY")
        || upper == "KEY"
}

fn is_pure_var_ref(value: &str) -> bool {
    let v = value.trim().trim_matches('"').trim_matches('\'');
    v.starts_with('$')
}

/// Parse `ENV` arguments — supports both `ENV KEY VALUE` and
/// `ENV KEY=VALUE [KEY2=VALUE2]` forms, returning owned pairs.
fn parse_env_pairs(body: &str) -> Vec<(String, String)> {
    let body = body.trim();
    if body.is_empty() {
        return Vec::new();
    }
    if body.contains('=') {
        let mut out = Vec::new();
        for token in body.split_whitespace() {
            if let Some((k, v)) = token.split_once('=') {
                out.push((k.to_string(), v.to_string()));
            }
        }
        return out;
    }
    // Legacy form: `ENV KEY rest-of-line...`
    let mut parts = body.splitn(2, char::is_whitespace);
    let key = parts.next().unwrap_or("").to_string();
    let value = parts.next().unwrap_or("").trim().to_string();
    if key.is_empty() {
        Vec::new()
    } else {
        vec![(key, value)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Dockerfile"), source))
    }

    #[test]
    fn flags_env_api_key() {
        assert_eq!(run("ENV API_KEY=sk-live-abc123").len(), 1);
    }

    #[test]
    fn flags_env_password_legacy_form() {
        assert_eq!(run("ENV DB_PASSWORD hunter2").len(), 1);
    }

    #[test]
    fn allows_non_secret_env() {
        assert!(run("ENV NODE_ENV=production").is_empty());
    }

    #[test]
    fn allows_var_passthrough() {
        assert!(run("ENV API_TOKEN=$API_TOKEN").is_empty());
    }
}

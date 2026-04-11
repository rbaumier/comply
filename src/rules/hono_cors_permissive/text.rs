use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn has_hono_cors_import(source: &str) -> bool {
    source.contains("hono/cors")
}

/// `cors()` with no arguments — defaults to `origin: '*'`.
fn is_bare_cors_call(line: &str) -> bool {
    if let Some(pos) = line.find("cors(") {
        let rest = &line[pos + 5..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with(')') {
            return true;
        }
    }
    false
}

/// `origin: '*'` or `origin: "*"`.
fn has_wildcard_origin(line: &str) -> bool {
    let normalized: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    normalized.contains("origin:'*'") || normalized.contains("origin:\"*\"")
}

/// `credentials: true` present on this line.
fn has_credentials_true(line: &str) -> bool {
    let normalized: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    normalized.contains("credentials:true")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !has_hono_cors_import(ctx.source) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            if is_bare_cors_call(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-cors-permissive".into(),
                    message: "`cors()` without arguments defaults to `origin: '*'` — any origin can access the API.".into(),
                    severity: Severity::Error,
                });
            }

            if has_wildcard_origin(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-cors-permissive".into(),
                    message: "`origin: '*'` allows any origin to access the API.".into(),
                    severity: Severity::Error,
                });
            }

            if has_credentials_true(line) {
                // Check surrounding context (current line + nearby lines) for a specific origin.
                let start = idx.saturating_sub(5);
                let end = (idx + 6).min(lines.len());
                let context = &lines[start..end];
                let has_specific_origin = context.iter().any(|l| {
                    let norm: String = l.chars().filter(|c| !c.is_whitespace()).collect();
                    (norm.contains("origin:") || norm.contains("origin :"))
                        && !norm.contains("origin:'*'")
                        && !norm.contains("origin:\"*\"")
                });
                if !has_specific_origin {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "hono-cors-permissive".into(),
                        message: "`credentials: true` without a specific origin — any origin can make credentialed requests.".into(),
                        severity: Severity::Error,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_bare_cors() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors());";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({ origin: '*' }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_credentials_without_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({\n  credentials: true\n}));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_specific_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({ origin: 'https://example.com' }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_credentials_with_specific_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({\n  origin: 'https://example.com',\n  credentials: true\n}));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.use(cors());";
        assert!(run(src).is_empty());
    }
}

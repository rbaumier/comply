//! hono-jwt-secret-hardcoded backend — flag `jwt({ secret: "..." })` literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_hono_jwt(source: &str) -> bool {
    source.contains("hono/jwt") || (source.contains("hono") && source.contains("jwt("))
}

/// Locate every `jwt(...)` call and return the byte offsets of any
/// `secret:` value that is a string literal.
fn find_hardcoded_secrets(source: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = source[search_from..].find("jwt(") {
        let start = search_from + rel;
        // Reject suffix matches like `setJwt(` — require a non-identifier byte before.
        if start > 0 {
            let prev = bytes[start - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' {
                search_from = start + 4;
                continue;
            }
        }
        let after = start + "jwt(".len();
        let mut depth = 1;
        let mut i = after;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        let mut body_end = i.saturating_sub(1);
        while body_end > after && !source.is_char_boundary(body_end) {
            body_end -= 1;
        }
        let body = &source[after..body_end];
        if let Some(secret_pos) = find_secret_literal(body) {
            let abs = after + secret_pos;
            let (line, col) = byte_to_line_col(source, abs);
            out.push((line, col));
        }
        search_from = i;
    }
    out
}

fn find_secret_literal(body: &str) -> Option<usize> {
    let key = "secret:";
    let mut from = 0usize;
    while let Some(rel) = body[from..].find(key) {
        let after = from + rel + key.len();
        let rest = &body[after..];
        let skip = rest.len() - rest.trim_start().len();
        let value_start = after + skip;
        match body.as_bytes().get(value_start).copied() {
            Some(b'"') | Some(b'\'') => return Some(value_start),
            // Backtick template with no `${` is also effectively hardcoded.
            Some(b'`') => {
                let tail = &body[value_start..];
                let end = tail[1..].find('`').map(|p| p + 1).unwrap_or(tail.len());
                if !tail[..end].contains("${") {
                    return Some(value_start);
                }
            }
            _ => {}
        }
        from = after;
    }
    None
}

fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_hono_jwt(ctx.source) {
            return Vec::new();
        }
        find_hardcoded_secrets(ctx.source)
            .into_iter()
            .map(|(line, column)| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`jwt({ secret })` is a hardcoded literal — load it from `env.JWT_SECRET` instead."
                    .into(),
                severity: Severity::Error,
                span: None,
            })
            .collect()
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
    fn flags_string_literal_secret() {
        let src = "import { jwt } from 'hono/jwt';\napp.use(jwt({ secret: 'my-secret' }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_double_quoted_secret() {
        let src = "import { jwt } from 'hono/jwt';\napp.use(jwt({ secret: \"sekret\" }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_env_secret() {
        let src = "import { jwt } from 'hono/jwt';\napp.use(jwt({ secret: env.JWT_SECRET }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_template_with_var() {
        let src = "import { jwt } from 'hono/jwt';\napp.use(jwt({ secret: `${process.env.S}` }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.use(jwt({ secret: 's' }));";
        assert!(run(src).is_empty());
    }
}

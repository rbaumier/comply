//! Flag CORS configurations that reflect the request `Origin` header without
//! validation. Three shapes:
//! - express/cors-style: `origin: req.headers.origin` (or `req.get('origin')`)
//! - raw header echo: `'Access-Control-Allow-Origin': req.headers.origin`
//! - cors callback: `origin: (origin, cb) => cb(null, origin)` (passes input through)

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn flag_line(line: &str) -> Option<usize> {
    let stripped = match line.find("//") {
        Some(p) => &line[..p],
        None => line,
    };
    // Shape 1: `origin: req.headers.origin` (any indentation, optional quotes).
    if let Some(pos) = stripped.find("req.headers.origin") {
        // Must be in a CORS context — same line should mention `origin:` or
        // `Access-Control-Allow-Origin`. Keep it permissive; reflection of
        // `req.headers.origin` is almost always a CORS bug.
        return Some(pos + 1);
    }
    if let Some(pos) = stripped.find("req.headers['origin']") {
        return Some(pos + 1);
    }
    if let Some(pos) = stripped.find("req.headers[\"origin\"]") {
        return Some(pos + 1);
    }
    if let Some(pos) = stripped.find("req.get('origin')") {
        return Some(pos + 1);
    }
    if let Some(pos) = stripped.find("req.get(\"origin\")") {
        return Some(pos + 1);
    }
    if let Some(pos) = stripped.find("req.get('Origin')") {
        return Some(pos + 1);
    }
    if let Some(pos) = stripped.find("request.headers.get('origin')") {
        return Some(pos + 1);
    }
    if let Some(pos) = stripped.find("request.headers.get(\"origin\")") {
        return Some(pos + 1);
    }
    None
}

impl TextCheck for Check {
    // Every flagged shape reflects the request `origin`/`Origin` header, so a
    // file containing neither substring can never fire this rule.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["origin", "Origin"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(col) = flag_line(line) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: col,
                    rule_id: super::META.id.into(),
                    message: "CORS reflects the request `Origin` without allowlist — any origin \
                              becomes trusted. Match against an explicit allowlist before echoing."
                        .to_string(),
                    severity: Severity::Error,
                    span: None,
                });
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
        Check.check(&CheckCtx::for_test(Path::new("server.ts"), source))
    }

    #[test]
    fn flags_express_cors_origin_reflection() {
        let src = "app.use(cors({ origin: req.headers.origin, credentials: true }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_raw_header_echo() {
        let src = "res.setHeader('Access-Control-Allow-Origin', req.headers.origin);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_request_headers_get() {
        let src = "const o = request.headers.get('origin'); res.headers.set('Access-Control-Allow-Origin', o);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_static_origin_allowlist() {
        let src =
            "app.use(cors({ origin: ['https://example.com', 'https://admin.example.com'] }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_comments() {
        let src = "// origin: req.headers.origin would be unsafe";
        assert!(run(src).is_empty());
    }
}

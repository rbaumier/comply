//! Flag CORS configurations that reflect the request `Origin` header back into a
//! CORS response sink without validation. A line is flagged only when an origin
//! read (`req.headers.origin`, `req.get('origin')`, …) flows into a sink on the
//! same line:
//! - express/cors-style: `origin: req.headers.origin` (value of an `origin:` key)
//! - raw header echo: `res.setHeader('Access-Control-Allow-Origin', req.headers.origin)`
//!
//! Reading the header for validation, logging, or sanitization (no sink) is safe
//! and is not flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

/// Ways the request `Origin` header is read.
const ORIGIN_READS: &[&str] = &[
    "req.headers.origin",
    "req.headers['origin']",
    "req.headers[\"origin\"]",
    "req.get('origin')",
    "req.get(\"origin\")",
    "req.get('Origin')",
    "request.headers.get('origin')",
    "request.headers.get(\"origin\")",
];

/// The read is a reflection only when its value lands in a CORS response sink on
/// the same line: an `Access-Control-Allow-Origin` response header, or the value of
/// an `origin:` cors-options key. A bare read used for validation, logging, or
/// sanitization (no sink) is not a reflection and is left alone.
fn is_reflected_into_cors_sink(stripped: &str, read_pos: usize) -> bool {
    if stripped.contains("Access-Control-Allow-Origin") {
        return true;
    }
    is_cors_origin_option_value(&stripped[..read_pos])
}

/// True when the text preceding the origin read is an `origin:` property key, i.e.
/// the read is assigned as that key's value (`cors({ origin: req.headers.origin })`).
/// Excludes `origin =` assignments and `origin ===` comparisons, which read into a
/// local rather than echoing into a cors options object.
fn is_cors_origin_option_value(before: &str) -> bool {
    let Some(without_colon) = before.trim_end().strip_suffix(':') else {
        return false;
    };
    let key = without_colon.trim_end();
    key.ends_with("origin") || key.ends_with("'origin'") || key.ends_with("\"origin\"")
}

fn flag_line(line: &str) -> Option<usize> {
    let stripped = match line.find("//") {
        Some(p) => &line[..p],
        None => line,
    };
    for read in ORIGIN_READS {
        if let Some(pos) = stripped.find(read)
            && is_reflected_into_cors_sink(stripped, pos)
        {
            return Some(pos + 1);
        }
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

    // Regression #2344: reading the origin to validate/sanitize it — with no CORS
    // response sink on the line — is defensive code, not reflection.
    #[test]
    fn allows_origin_read_for_validation() {
        let src = "\
const isOriginInvalid = checkInvalidHeaderChar(req.headers.origin);
if (isOriginInvalid) {
  const origin = req.headers.origin;
  req.headers.origin = null;
  return fn(Server.errors.BAD_REQUEST, { name: \"INVALID_ORIGIN\", origin });
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_origin_read_stored_in_local() {
        let src = "const requestOrigin = req.headers.origin;";
        assert!(run(src).is_empty());
    }

    // Negative-space guard: the genuine reflection vulnerability must still fire.
    #[test]
    fn still_flags_setheader_reflection() {
        let src = "res.setHeader('Access-Control-Allow-Origin', req.headers.origin);";
        assert_eq!(run(src).len(), 1);
    }
}

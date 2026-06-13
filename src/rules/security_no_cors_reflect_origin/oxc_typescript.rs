use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn flag_line(line: &str) -> Option<usize> {
    let stripped = match line.find("//") {
        Some(p) => &line[..p],
        None => line,
    };
    if let Some(pos) = stripped.find("req.headers.origin") {
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

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["origin", "Origin"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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

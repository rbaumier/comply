//! no-ssrf-fetch OXC backend — flag server-side outbound HTTP calls
//! whose first argument references request-scoped data.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const DIRECT_FN_NAMES: &[&str] = &["fetch", "got", "request", "ky"];

/// Methods on an HTTP client/module that issue an OUTBOUND request. Server and
/// listener constructors (`createServer`, `Server`, `createSecureServer`, …) are
/// deliberately absent: they create inbound listeners, never an SSRF sink.
const OUTBOUND_REQUEST_METHODS: &[&str] =
    &["get", "post", "put", "delete", "patch", "head", "request"];

const HTTP_MODULE_ROOTS: &[&str] = &["axios", "http", "https", "got", "ky", "needle"];

const USER_DATA_NEEDLES: &[&str] = &[
    "req.query",
    "req.params",
    "req.body",
    "request.query",
    "request.params",
    "request.body",
    "searchParams.get",
];

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::StaticMemberExpression(m) => {
            let obj = callee_name(&m.object)?;
            Some(format!("{}.{}", obj, m.property.name))
        }
        Expression::Identifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

fn is_outbound_http_call(name: &str) -> bool {
    if DIRECT_FN_NAMES.contains(&name) {
        return true;
    }
    if let Some((receiver, method)) = name.rsplit_once('.')
        && OUTBOUND_REQUEST_METHODS.contains(&method)
        && HTTP_MODULE_ROOTS.iter().any(|r| receiver.ends_with(r))
    {
        return true;
    }
    false
}

fn arg_references_user_data(text: &str) -> bool {
    USER_DATA_NEEDLES.iter().any(|n| text.contains(n))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Some(name) = callee_name(&call.callee) else { return };
        if !is_outbound_http_call(&name) {
            return;
        }

        let Some(first) = call.arguments.first() else { return };
        let span = first.span();
        let text = &ctx.source[span.start as usize..span.end as usize];
        if arg_references_user_data(text) {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Outbound HTTP URL built from user input \u{2014} validate against a host allowlist before sending.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_fetch_with_req_query() {
        assert_eq!(run_on("fetch(req.query.target)").len(), 1);
    }

    #[test]
    fn flags_axios_get_with_body() {
        assert_eq!(run_on("axios.get(req.body.url)").len(), 1);
    }

    #[test]
    fn flags_fetch_with_search_params() {
        assert_eq!(run_on("fetch(searchParams.get('next'))").len(), 1);
    }

    #[test]
    fn allows_fetch_with_literal() {
        assert!(run_on("fetch('https://api.example.com/data')").is_empty());
    }

    #[test]
    fn allows_fetch_with_internal_variable() {
        assert!(run_on("fetch(safeUrl)").is_empty());
    }

    #[test]
    fn allows_http_create_server_with_request_handler() {
        // http.createServer creates an inbound listener, not an outbound request,
        // so it can never be an SSRF sink even when the handler touches req data.
        let src = r#"const httpServer = http.createServer((req, res) => {
            const _req = req;
            _req.query = opts.query;
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_https_create_secure_server() {
        assert!(run_on("https.createSecureServer((req, res) => { res.end(req.url); })").is_empty());
    }

    #[test]
    fn flags_http_get_with_user_url() {
        assert_eq!(run_on("http.get(req.query.target)").len(), 1);
    }

    #[test]
    fn flags_https_request_with_user_url() {
        assert_eq!(run_on("https.request(req.body.url)").len(), 1);
    }
}

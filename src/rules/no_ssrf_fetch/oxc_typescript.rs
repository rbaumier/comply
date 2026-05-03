//! no-ssrf-fetch OXC backend — flag server-side outbound HTTP calls
//! whose first argument references request-scoped data.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const DIRECT_FN_NAMES: &[&str] = &["fetch", "got", "request", "ky"];

const AXIOS_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "request"];

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
    if let Some((receiver, method)) = name.rsplit_once('.') {
        if HTTP_MODULE_ROOTS.contains(&receiver) {
            return true;
        }
        if AXIOS_METHODS.contains(&method)
            && HTTP_MODULE_ROOTS.iter().any(|r| receiver.ends_with(r))
        {
            return true;
        }
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}

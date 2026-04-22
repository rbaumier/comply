//! no-ssrf-fetch backend — flag server-side outbound HTTP calls
//! (`fetch`, `axios.get/post/...`, `got`, `http.get`, `https.request`)
//! whose first argument references request-scoped data.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !is_outbound_http_call(name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else { return };
    let Ok(text) = first.utf8_text(source) else { return };
    if arg_references_user_data(text) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "no-ssrf-fetch",
            "Outbound HTTP URL built from user input — validate against a host allowlist before sending.".into(),
            Severity::Error,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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

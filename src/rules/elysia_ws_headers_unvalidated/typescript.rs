//! elysia-ws-headers-unvalidated backend — flag `.ws(` reading headers without `headers:` schema.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".ws") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    let reads_headers = args_text.contains("headers.authorization")
        || args_text.contains("headers['authorization']")
        || args_text.contains("headers[\"authorization\"]")
        || args_text.contains("headers.cookie");
    if !reads_headers {
        return;
    }

    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if norm.contains("headers:t.") || norm.contains("header:t.") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-ws-headers-unvalidated".into(),
        message: "WebSocket route reads request headers but declares no `headers:` schema — header presence is not enforced.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_headers_read_without_schema() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().ws('/chat', { beforeHandle({ headers }) { const t = headers.authorization; } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_headers_with_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().ws('/chat', { headers: t.Object({ authorization: t.String() }), beforeHandle({ headers }) { const x = headers.authorization; } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.ws('/chat', { beforeHandle({ headers }) { const t = headers.authorization; } });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

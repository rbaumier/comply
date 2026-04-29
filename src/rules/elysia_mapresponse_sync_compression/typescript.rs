//! elysia-mapresponse-sync-compression backend — flag sync compression in mapResponse.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["deflateSync", "gzipSync"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "mapResponse" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    if !args_text.contains("gzipSync") && !args_text.contains("deflateSync") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-mapresponse-sync-compression".into(),
        message: "`mapResponse` is on the hot path — synchronous `gzipSync` / `deflateSync` blocks the event loop. Use the async `zlib/promises` variants.".into(),
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
    fn flags_gzip_sync() {
        let src = "import { Elysia } from 'elysia';\napp.mapResponse(({ response }) => gzipSync(response));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_deflate_sync() {
        let src = "import { Elysia } from 'elysia';\napp.mapResponse(({ response }) => deflateSync(Buffer.from(response)));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_gzip() {
        let src = "import { Elysia } from 'elysia';\nimport { gzip } from 'zlib/promises';\napp.mapResponse(async ({ response }) => await gzip(response));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.mapResponse(({ response }) => gzipSync(response));";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

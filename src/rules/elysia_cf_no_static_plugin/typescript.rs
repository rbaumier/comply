//! elysia-cf-no-static-plugin backend — flag `staticPlugin` or `.file(` under `CloudflareAdapter`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    let last = callee_text.rsplit('.').next().unwrap_or("");
    let is_static_plugin = callee_text == "staticPlugin";
    let is_file = last == "file" && callee_text.contains('.');
    if !is_static_plugin && !is_file {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cf-no-static-plugin".into(),
        message: "Filesystem-backed static serving (`staticPlugin` / `.file()`) does not work under Cloudflare — use the `[assets]` binding.".into(),
        severity: Severity::Error,
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
    fn flags_static_plugin() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nimport { staticPlugin } from '@elysiajs/static';\napp.use(staticPlugin());";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_file_helper() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\napp.get('/img', ({ file }) => file('logo.png'));\napp.file('img.png');";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_cf_files() {
        let src = "import { staticPlugin } from '@elysiajs/static';\napp.use(staticPlugin());";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

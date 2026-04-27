//! elysia-cf-env-import backend — flag `process.env` under `CloudflareAdapter`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    if !text.starts_with("process.env") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cf-env-import".into(),
        message: "`process.env` is undefined on Cloudflare Workers — use `import { env } from 'cloudflare:workers'`.".into(),
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
    fn flags_process_env_under_cf() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nconst secret = process.env.SECRET;";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_cloudflare_workers_env() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nimport { env } from 'cloudflare:workers';\nconst secret = env.SECRET;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_cf_files() {
        let src = "const secret = process.env.SECRET;";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

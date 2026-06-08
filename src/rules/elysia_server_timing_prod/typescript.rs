//! elysia-server-timing-prod backend — flag `serverTiming({ enabled: true })` literals.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["\"serverTiming\""] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "serverTiming" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if !norm.contains("enabled:true") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-server-timing-prod".into(),
        message: "`serverTiming({ enabled: true })` is unconditional — gate it on an env flag.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_enabled_true_literal() {
        let src = "import { serverTiming } from '@elysiajs/server-timing';\napp.use(serverTiming({ enabled: true }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_env_gated_enabled() {
        let src = "import { serverTiming } from '@elysiajs/server-timing';\napp.use(serverTiming({ enabled: process.env.NODE_ENV !== 'production' }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_server_timing_files() {
        let src = "serverTiming({ enabled: true });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}

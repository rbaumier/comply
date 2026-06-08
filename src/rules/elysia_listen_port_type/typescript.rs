//! elysia-listen-port-type backend — flag `.listen(process.env.PORT)` without a numeric coercion.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["process.env.PORT"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "listen" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    let first_text = first.utf8_text(source).unwrap_or("");
    if !first_text.contains("process.env.PORT") {
        return;
    }
    if first_text.contains("Number(") || first_text.contains("parseInt") || first_text.contains("+process.env.PORT") {
        return;
    }
    if first_text.contains("??") || first_text.contains("||") {
        // E.g. `process.env.PORT ?? 3000` — still a string when env is set, but commonly tolerated.
        // We only skip when wrapped in a coercion above.
    }

    // The only remaining cases are unwrapped uses.
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-listen-port-type".into(),
        message: "`.listen(process.env.PORT)` passes a string — wrap with `Number(...)` or `parseInt(...)`.".into(),
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
    fn flags_raw_env_port() {
        let src = "import { Elysia } from 'elysia';\napp.listen(process.env.PORT);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_env_port_with_fallback_string() {
        let src = "import { Elysia } from 'elysia';\napp.listen(process.env.PORT ?? '3000');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_number_coercion() {
        let src = "import { Elysia } from 'elysia';\napp.listen(Number(process.env.PORT));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_parseint_coercion() {
        let src = "import { Elysia } from 'elysia';\napp.listen(parseInt(process.env.PORT ?? '3000', 10));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.listen(process.env.PORT);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}

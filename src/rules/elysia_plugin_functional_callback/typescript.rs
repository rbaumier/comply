//! elysia-plugin-functional-callback backend — flag arrow plugins typed as `(app: Elysia) => ...`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["arrow_function", "function_declaration", "function_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(params) = node.child_by_field_name("parameters") else { return };
    let params_text = params.utf8_text(source).unwrap_or("");
    // Single param annotated with `: Elysia`.
    if !params_text.contains(": Elysia") && !params_text.contains(":Elysia") {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };
    let body_text = body.utf8_text(source).unwrap_or("");
    // Heuristic: the body chains methods on the parameter (e.g. `app.get(...)`).
    // Extract param name from `(name: Elysia)`.
    let pname = params_text
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(':')
        .next()
        .unwrap_or("")
        .trim();
    if pname.is_empty() {
        return;
    }
    let chain_marker = format!("{pname}.");
    if !body_text.contains(&chain_marker) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-plugin-functional-callback".into(),
        message: "Functional plugin `(app: Elysia) => ...` loses type inference — return a `new Elysia()` instance instead.".into(),
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
    fn flags_arrow_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = (app: Elysia) => app.get('/x', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_function_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport function plugin(app: Elysia) { return app.get('/x', () => 'ok'); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_instance_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia({ name: 'plugin' }).get('/x', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "export const plugin = (app: Elysia) => app.get('/x', () => 'ok');";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}

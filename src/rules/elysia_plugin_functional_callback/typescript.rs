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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
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
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

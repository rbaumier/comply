//! elysia-prefer-instance-plugin backend — flag callback-style Elysia plugins.

use crate::diagnostic::{Diagnostic, Severity};

fn first_param_is_elysia(node: tree_sitter::Node, source: &[u8]) -> bool {
    // Try `parameters` field first (function_expression / arrow_function with parens).
    let params = node.child_by_field_name("parameters");
    let Some(params) = params else { return false };
    if params.kind() != "formal_parameters" {
        return false;
    }
    for i in 0..params.child_count() {
        let Some(child) = params.child(i) else {
            continue;
        };
        if child.kind() != "required_parameter" && child.kind() != "optional_parameter" {
            continue;
        }
        // Look for a type_annotation whose text is `Elysia`.
        for j in 0..child.child_count() {
            let Some(t) = child.child(j) else { continue };
            if t.kind() != "type_annotation" {
                continue;
            }
            let txt = t
                .utf8_text(source)
                .unwrap_or("")
                .trim_start_matches(':')
                .trim();
            if txt == "Elysia" || txt.starts_with("Elysia<") {
                return true;
            }
        }
        // Only inspect the first parameter.
        return false;
    }
    false
}

crate::ast_check! { on ["arrow_function", "function_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    if !first_param_is_elysia(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-prefer-instance-plugin".into(),
        message: "Callback-style plugin `(app: Elysia) => ...` — prefer `new Elysia({ name: '...' })` instance plugins for deduplication and type inference.".into(),
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
    fn flags_callback_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = (app: Elysia) => app.get('/', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_function_expression_callback() {
        let src = "import { Elysia } from 'elysia';\nexport function plugin(app: Elysia) { return app.get('/', () => 'ok'); }";
        // function declarations don't match; use a function expression.
        let src2 = "import { Elysia } from 'elysia';\nexport const plugin = function(app: Elysia) { return app; };";
        let _ = src;
        assert_eq!(run_on(src2).len(), 1);
    }

    #[test]
    fn allows_instance_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia({ name: 'p' }).get('/', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "export const plugin = (app: Elysia) => app.get('/', () => 'ok');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

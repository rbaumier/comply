//! elysia-no-context-type backend — flag manual `Context` type annotations.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["required_parameter", "optional_parameter"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    // Walk the param's children looking for a type_annotation whose payload is `Context`.
    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else { continue };
        if child.kind() != "type_annotation" {
            continue;
        }
        // type_annotation: ":" type
        for j in 0..child.child_count() {
            let Some(t) = child.child(j) else { continue };
            if t.kind() == ":" {
                continue;
            }
            let text = t.utf8_text(source).unwrap_or("").trim();
            if text == "Context" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "elysia-no-context-type".into(),
                    message: "Parameter typed as `Context` — Elysia infers the context type per-route. Destructure inline instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_context_param() {
        let src = "import { Context } from 'elysia';\nfunction h(ctx: Context) { return 1; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_context_arrow_param() {
        let src = "import { Elysia } from 'elysia';\nconst h = (context: Context) => 1;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_destructured_param() {
        let src = "import { Elysia } from 'elysia';\nconst h = ({ body, set }) => 1;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "function h(ctx: Context) { return 1; }";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

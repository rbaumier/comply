//! elysia-deploy-port-hardcoded backend — flag `.listen(<number>)` numeric literals.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".listen") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // First positional argument.
    let mut cursor = args.walk();
    let mut first_arg = None;
    for child in args.named_children(&mut cursor) {
        first_arg = Some(child);
        break;
    }
    let Some(first) = first_arg else { return };
    if first.kind() != "number" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-deploy-port-hardcoded".into(),
        message: "Hardcoded port in `.listen()` — read from `process.env.PORT` so deploy platforms can override it.".into(),
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
    fn flags_numeric_port() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_env_port() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().listen(process.env.PORT ?? 3000);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "server.listen(3000);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

//! elysia-require-method-chaining backend — flag broken Elysia method chains.

use crate::diagnostic::{Diagnostic, Severity};

const ELYSIA_METHODS: &[&str] = &[
    "state",
    "get",
    "post",
    "put",
    "patch",
    "delete",
    "use",
    "guard",
    "onBeforeHandle",
    "onAfterHandle",
    "onError",
    "onRequest",
    "derive",
    "resolve",
    "decorate",
    "model",
    "listen",
];

crate::ast_check! { on ["expression_statement"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    // expression_statement -> call_expression
    let Some(call) = node.child(0) else { return };
    if call.kind() != "call_expression" {
        return;
    }

    let Some(callee) = call.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(object) = callee.child_by_field_name("object") else { return };
    // In a proper chain, object is a call_expression. If it's an identifier,
    // the chain has been broken (the method was called on a stored variable).
    if object.kind() != "identifier" {
        return;
    }

    let Some(property) = callee.child_by_field_name("property") else { return };
    let prop_text = property.utf8_text(source).unwrap_or("");
    if !ELYSIA_METHODS.contains(&prop_text) {
        return;
    }

    let pos = call.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-require-method-chaining".into(),
        message: format!(
            "`{}.{}(...)` breaks the Elysia method chain — type inference is lost. Chain methods on `new Elysia()` directly.",
            object.utf8_text(source).unwrap_or("app"),
            prop_text
        ),
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
    fn flags_broken_chain() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia();\napp.get('/', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_broken_state_call() {
        let src =
            "import { Elysia } from 'elysia';\nconst app = new Elysia();\napp.state('count', 0);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_proper_chain() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().state('count', 0).get('/', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const app = new Hono();\napp.get('/', () => 'ok');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

//! elysia-inline-handlers backend — flag route handlers passed by reference.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(object) = callee.child_by_field_name("object") else { return };
    // Skip calls on well-known built-in objects (e.g. Reflect.get, Object.assign).
    let object_text = object.utf8_text(source).unwrap_or("");
    if matches!(object_text, "Reflect" | "Object" | "Array" | "Math" | "Promise" | "JSON") {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    let prop_text = property.utf8_text(source).unwrap_or("");
    if !ROUTE_METHODS.contains(&prop_text) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };

    // Collect non-syntax (non-paren, non-comma) children — these are the actual args.
    let mut real_args: Vec<tree_sitter::Node> = Vec::new();
    for i in 0..args.child_count() {
        let Some(child) = args.child(i) else { continue };
        let kind = child.kind();
        if kind == "(" || kind == ")" || kind == "," {
            continue;
        }
        real_args.push(child);
    }

    if real_args.len() < 2 {
        return;
    }

    let handler = real_args[1];
    let kind = handler.kind();
    // Inline handlers are arrow functions or function expressions.
    if kind == "arrow_function" || kind == "function_expression" || kind == "function" {
        return;
    }
    // Also allow string literals (static responses) and other literals — they're not handler refs.
    if kind == "string"
        || kind == "number"
        || kind == "true"
        || kind == "false"
        || kind == "null"
        || kind == "object"
        || kind == "array"
        || kind == "template_string"
    {
        return;
    }
    // Identifier or member_expression here means a handler-by-reference.
    if kind != "identifier" && kind != "member_expression" {
        return;
    }

    let pos = handler.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-inline-handlers".into(),
        message: "Route handler passed by reference loses Elysia's type inference. Wrap in an inline arrow function.".into(),
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
    fn flags_handler_by_identifier() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', handleFn);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_handler_by_member_expression() {
        let src = "import { Elysia } from 'elysia';\napp.post('/', Controller.method);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_inline_arrow() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ body }) => doThing(body));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_string() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/', handleFn);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn no_fp_on_reflect_get() {
        // Reflect.get(source, prop) — second arg is a property key, not a route handler.
        let src = r#"import { Elysia } from 'elysia';
const handler: ProxyHandler<SomeType> = {
  get(_target, prop) {
    const result: unknown = Reflect.get(source, prop);
    return result;
  },
};"#;
        assert!(run_on(src).is_empty());
    }
}

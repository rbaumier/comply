//! elysia-guard-overrides-route-schema — flag a `.guard({ body: ... }, ...)`
//! whose callback body contains a route call (`.get/.post/.put/.delete/.patch`)
//! that also passes a schema object literal containing `body:`.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "all", "head", "options"];

fn arguments_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    let Some(args) = node.child_by_field_name("arguments") else { return "" };
    args.utf8_text(source).unwrap_or("")
}

fn callee_property<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<String> {
    let callee = node.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let property = callee.child_by_field_name("property")?;
    Some(property.utf8_text(source).unwrap_or("").to_string())
}

/// Walk descendants and find any route call that has `body:` in its arguments.
fn route_call_with_body<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some(name) = callee_property(child, source) {
                if ROUTE_METHODS.contains(&name.as_str()) {
                    let args = arguments_text(child, source);
                    if args.contains("body:") || args.contains("body :") {
                        return Some(child);
                    }
                }
            }
        }
        if let Some(found) = route_call_with_body(child, source) {
            return Some(found);
        }
    }
    None
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    let Some(name) = callee_property(node, source) else { return };
    if name != "guard" {
        return;
    }
    // Inspect only the FIRST argument (the schema object). Its body must
    // contain `body:` for the guard to be schema-bearing — otherwise the
    // guard isn't redefining a body schema and the rule does not apply.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut acursor = args.walk();
    let Some(first) = args.named_children(&mut acursor).next() else { return };
    let first_text = first.utf8_text(source).unwrap_or("");
    if !(first_text.contains("body:") || first_text.contains("body :")) {
        return;
    }
    if let Some(inner) = route_call_with_body(node, source) {
        let pos = inner.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "elysia-guard-overrides-route-schema".into(),
            message: "Route inside `.guard({ body: ... })` redeclares `body:` — the inner schema silently overrides the guard.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_inner_body_overriding_guard() {
        let src = "import { Elysia } from 'elysia';\napp.guard({ body: t.Object({ a: t.String() }) }, (g) => g.post('/x', () => 'ok', { body: t.Object({ a: t.Number() }) }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_guard_without_inner_body() {
        let src = "import { Elysia } from 'elysia';\napp.guard({ body: t.Object({ a: t.String() }) }, (g) => g.post('/x', () => 'ok'));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_guard_without_body() {
        let src = "import { Elysia } from 'elysia';\napp.guard({ headers: t.Object({}) }, (g) => g.post('/x', () => 'ok', { body: t.Object({}) }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.guard({ body: 1 }, (g) => g.post('/x', () => 1, { body: 2 }));";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

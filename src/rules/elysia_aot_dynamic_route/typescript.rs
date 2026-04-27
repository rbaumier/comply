//! elysia-aot-dynamic-route — flag `.get(<dynamic>, ...)`/`.post(...)`/etc.
//! when the first argument is a template_string with substitutions or a
//! binary_expression (string concatenation) instead of a plain string.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "all", "head", "options"];

fn is_dynamic_path(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    match node.kind() {
        "template_string" => {
            // Plain backtick string with no `${...}` is fine.
            let text = node.utf8_text(source).unwrap_or("");
            text.contains("${")
        }
        "binary_expression" => {
            // `'/users/' + id` — concatenation.
            let text = node.utf8_text(source).unwrap_or("");
            text.contains('+')
        }
        _ => false,
    }
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let name = prop.utf8_text(source).unwrap_or("");
    if !ROUTE_METHODS.contains(&name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else { return };
    if !is_dynamic_path(first, source) {
        return;
    }
    let pos = first.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-aot-dynamic-route".into(),
        message: "Route path built dynamically (template literal / concatenation) — Elysia AOT can only compile static path strings. Use `:param` segments instead.".into(),
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
    fn flags_template_literal_with_substitution() {
        let src = "import { Elysia } from 'elysia';\napp.get(`/users/${id}`, () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_string_concatenation() {
        let src = "import { Elysia } from 'elysia';\napp.post('/users/' + id, () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_static_string() {
        let src = "import { Elysia } from 'elysia';\napp.get('/users/:id', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_plain_template_string() {
        let src = "import { Elysia } from 'elysia';\napp.get(`/users/:id`, () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get(`/users/${id}`, () => 'ok');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}

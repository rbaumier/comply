//! elysia-no-body-on-get backend — flag `.get`/`.head` calls that try to
//! validate a request body.

use crate::diagnostic::{Diagnostic, Severity};

const BODYLESS_METHODS: &[&str] = &["get", "head"];

/// Walk the call's `arguments` node and return true if any argument that is
/// an object literal contains a `body:` key with a non-empty value.
/// Function expressions (the handler) are skipped so destructure renames
/// like `({body:b}) => ...` aren't mistaken for a schema key.
fn options_has_body_key(args: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        if arg.kind() != "object" {
            continue;
        }
        let mut c2 = arg.walk();
        for prop in arg.named_children(&mut c2) {
            if prop.kind() != "pair" {
                continue;
            }
            let Some(key) = prop.child_by_field_name("key") else {
                continue;
            };
            let key_text = key.utf8_text(source).unwrap_or("");
            if key_text != "body" {
                continue;
            }
            let Some(value) = prop.child_by_field_name("value") else {
                continue;
            };
            let value_text = value.utf8_text(source).unwrap_or("");
            if !value_text.trim().is_empty() {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let prop_text = prop.utf8_text(source).unwrap_or("");
    if !BODYLESS_METHODS.contains(&prop_text) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };

    // Body schema present? Any `body:` key with a non-empty value counts —
    // schemas may be `t.X`, model refs, imported TypeBox constants, etc.
    if !options_has_body_key(args, source) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-no-body-on-get".into(),
        message: "`.get()` and `.head()` cannot carry a request body — move validation to `query:` or use `.post()`.".into(),
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
    fn flags_get_with_body_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { body: t.Object({ a: t.String() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_head_with_body_model_ref() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().head('/x', () => 'ok', { body: 'model.x' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_get_with_query_only() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { query: t.Object({ q: t.String() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/x', () => 'ok', { body: t.Object({}) });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }

    #[test]
    fn flags_get_with_typed_reference_body() {
        let src = "import { Elysia } from 'elysia';\nimport { UserSchema } from './schemas';\nnew Elysia().get('/x', () => 'ok', { body: UserSchema });";
        assert_eq!(run_on(src).len(), 1);
    }
}

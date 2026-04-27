//! elysia-route-missing-body-schema backend — flag `.post/.put/.patch` routes
//! whose handler destructures `body` but options carry no `body:` schema.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &["post", "put", "patch"];

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
            let Some(key) = prop.child_by_field_name("key") else { continue };
            let key_text = key.utf8_text(source).unwrap_or("");
            if key_text != "body" {
                continue;
            }
            let Some(value) = prop.child_by_field_name("value") else { continue };
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
    if !ROUTE_METHODS.contains(&prop_text) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

    // Handler destructures `body` — look for `{body` or `({body`.
    let handler_uses_body = norm.contains("({body")
        || norm.contains(",{body")
        || norm.contains("{body,")
        || norm.contains("{body}")
        || norm.contains("{body:");

    if !handler_uses_body { return; }

    // Body schema present? Walk the arguments and inspect the options object —
    // any `body:` key with a non-empty value counts (`t.X`, model refs, imported
    // TypeBox constants like `UserSchema`, etc.). Looking only inside the options
    // object avoids confusing handler destructure renames (`{body:b}`) with a schema.
    if options_has_body_key(args, source) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-route-missing-body-schema".into(),
        message: "Route reads `body` but has no `body:` schema in options — Elysia will not validate the payload.".into(),
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
    fn flags_post_with_body_no_schema() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_post_with_body_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: t.Object({ a: t.String() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_post_with_model_ref() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().post('/x', ({ body }) => body, { body: 'user.create' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.post('/x', ({ body }) => body);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }

    #[test]
    fn allows_post_with_typed_reference() {
        let src = "import { Elysia } from 'elysia';\nimport { UserSchema } from './schemas';\nnew Elysia().post('/x', ({ body }) => body, { body: UserSchema });";
        assert!(run_on(src).is_empty());
    }
}

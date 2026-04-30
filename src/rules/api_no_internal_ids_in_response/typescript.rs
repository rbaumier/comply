//! Scan interface / type-alias declarations whose name matches a response
//! suffix and flag property names that look internal:
//!   - ends with `_id` (snake_case FK leak)
//!   - starts with `internal_` / `internal`
//!   - exactly `pk` or `rowid`

use crate::diagnostic::{Diagnostic, Severity};

const RESPONSE_SUFFIXES: &[&str] = &[
    "Response", "Dto", "DTO", "Payload", "Reply", "Result", "Body", "Output", "View",
];

fn is_response_type(name: &str) -> bool {
    RESPONSE_SUFFIXES.iter().any(|s| name.ends_with(s))
}

fn is_internal_field(name: &str) -> bool {
    if name == "pk" || name == "rowid" || name == "oid" {
        return true;
    }
    if name.starts_with("internal_") || name.starts_with("internal") && name.len() > 8 {
        // "internal" prefix when followed by an uppercase letter or underscore.
        let rest = &name[8..];
        if rest.starts_with('_') || rest.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            return true;
        }
    }
    // snake_case foreign-key leakage: `user_id`, `order_id`, ...
    if name.ends_with("_id") && name.len() > 3 {
        return true;
    }
    false
}

fn push_internal_props(
    body: tree_sitter::Node,
    source: &[u8],
    ctx_path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() != "property_signature" {
            continue;
        }
        let Some(name_node) = member.child_by_field_name("name") else {
            continue;
        };
        let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else {
            continue;
        };
        if !is_internal_field(name) {
            continue;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx_path,
            &member,
            super::META.id,
            format!(
                "Response field `{name}` looks internal — rename to its public form or drop it from the DTO."
            ),
            Severity::Warning,
        ));
    }
}

crate::ast_check! { on ["interface_declaration", "type_alias_declaration"] => |node, source, ctx, diagnostics|
match node.kind() {
        "interface_declaration" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else { return };
            if !is_response_type(name) { return }
            let Some(body) = node.child_by_field_name("body") else { return };
            push_internal_props(body, source, ctx.path, diagnostics);
        }
        "type_alias_declaration" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else { return };
            if !is_response_type(name) { return }
            let Some(value) = node.child_by_field_name("value") else { return };
            if value.kind() != "object_type" { return }
            push_internal_props(value, source, ctx.path, diagnostics);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_snake_case_foreign_key() {
        let d = run("interface OrderResponse { user_id: string; total: number }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("user_id"));
    }

    #[test]
    fn flags_pk_field() {
        let d = run("interface UserDto { pk: number; name: string }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_internal_prefixed_field() {
        let d = run("interface AccountResponse { internal_tier: string; name: string }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_camelcase_id() {
        assert!(run("interface OrderResponse { userId: string; total: number }").is_empty());
    }

    #[test]
    fn allows_plain_id() {
        assert!(run("interface UserResponse { id: string; name: string }").is_empty());
    }

    #[test]
    fn ignores_non_response_types() {
        assert!(run("interface UserRow { user_id: string; pk: number }").is_empty());
    }
}

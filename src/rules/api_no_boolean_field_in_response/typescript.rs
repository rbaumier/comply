//! Walk interface / type-alias declarations whose name matches a
//! response-shape suffix, then flag every `boolean` property signature
//! inside.
//!
//! Tree-sitter shapes:
//!
//! ```ignore
//! interface_declaration {
//!   name: type_identifier,
//!   body: interface_body {
//!     property_signature {
//!       name: property_identifier,
//!       type: type_annotation { predefined_type "boolean" }
//!     }
//!   }
//! }
//!
//! type_alias_declaration {
//!   name: type_identifier,
//!   value: object_type {
//!     property_signature { ... }
//!   }
//! }
//! ```

use crate::diagnostic::{Diagnostic, Severity};

const RESPONSE_SUFFIXES: &[&str] = &[
    "Response", "Dto", "DTO", "Payload", "Reply", "Result", "Body",
];

fn looks_like_response_type(name: &str) -> bool {
    RESPONSE_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// Return `true` if the `type_annotation` node wraps a bare `boolean`
/// predefined type. Ignores `boolean | null`, `boolean[]`, etc. — those
/// already hint at a richer state space.
fn is_plain_boolean(type_annotation: tree_sitter::Node) -> bool {
    let mut cursor = type_annotation.walk();
    for child in type_annotation.children(&mut cursor) {
        if child.kind() == "predefined_type" {
            // predefined_type's single child is the keyword token.
            let mut tc = child.walk();
            for kw in child.children(&mut tc) {
                if kw.kind() == "boolean" {
                    return true;
                }
            }
        }
    }
    false
}

fn push_boolean_props(
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
        let Some(type_ann) = member.child_by_field_name("type") else {
            continue;
        };
        if !is_plain_boolean(type_ann) {
            continue;
        }
        let prop_name = member
            .child_by_field_name("name")
            .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
            .unwrap_or("<field>");
        let pos = member.start_position();
        diagnostics.push(Diagnostic {
            path: ctx_path.to_path_buf().into(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "api-no-boolean-field-in-response".into(),
            message: format!(
                "Response field `{prop_name}: boolean` is not extensible — prefer a string-union / enum so new states don't break clients."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

crate::ast_check! { on ["interface_declaration", "type_alias_declaration"] => |node, source, ctx, diagnostics|
match node.kind() {
        "interface_declaration" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else { return };
            if !looks_like_response_type(name) { return }
            let Some(body) = node.child_by_field_name("body") else { return };
            push_boolean_props(body, source, ctx.path, diagnostics);
        }
        "type_alias_declaration" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else { return };
            if !looks_like_response_type(name) { return }
            let Some(value) = node.child_by_field_name("value") else { return };
            if value.kind() != "object_type" { return }
            push_boolean_props(value, source, ctx.path, diagnostics);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_boolean_in_response_interface() {
        let d = run_on("interface UserResponse { id: string; isActive: boolean }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("isActive"));
    }

    #[test]
    fn flags_boolean_in_dto_type_alias() {
        let d = run_on("type OrderDto = { id: string; isPaid: boolean };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("isPaid"));
    }

    #[test]
    fn flags_multiple_boolean_fields() {
        let d = run_on(
            "interface AccountPayload { isActive: boolean; isVerified: boolean; name: string }",
        );
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_boolean_in_non_response_type() {
        assert!(run_on("interface UserModel { isActive: boolean }").is_empty());
    }

    #[test]
    fn allows_string_union_in_response() {
        assert!(
            run_on("interface UserResponse { id: string; status: 'active' | 'inactive' }")
                .is_empty()
        );
    }

    #[test]
    fn allows_non_boolean_fields() {
        assert!(run_on("interface UserResponse { id: string; name: string }").is_empty());
    }
}

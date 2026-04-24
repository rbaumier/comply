//! Walk interface / type-alias declarations. If a declaration contains
//! server-managed fields (`id`, `createdAt`, `updatedAt`) AND its name
//! suggests shared input/output use (generic names like `User`, `Order`,
//! or explicit `*Input`, `*Request` with server fields), flag it.
//!
//! Heuristic: an interface is suspicious when it has server-managed
//! fields AND its name is a bare entity name (no `Response`/`Output`
//! suffix) — such types tend to end up reused in request bodies.

use crate::diagnostic::{Diagnostic, Severity};

const SERVER_MANAGED_FIELDS: &[&str] = &[
    "id",
    "createdAt",
    "updatedAt",
    "created_at",
    "updated_at",
    "deletedAt",
    "deleted_at",
];

const OUTPUT_SUFFIXES: &[&str] = &[
    "Response",
    "Output",
    "Dto",
    "DTO",
    "Result",
    "Reply",
    "Payload",
    "View",
    "Entity",
    "Model",
    "Row",
    "Record",
];

const INPUT_SUFFIXES: &[&str] = &[
    "Input", "Request", "Create", "Update", "Patch", "Args", "Params", "Body",
];

fn has_output_suffix(name: &str) -> bool {
    OUTPUT_SUFFIXES.iter().any(|s| name.ends_with(s))
}

fn has_input_suffix(name: &str) -> bool {
    INPUT_SUFFIXES.iter().any(|s| name.ends_with(s))
}

fn collect_prop_names<'a>(
    body: tree_sitter::Node<'a>,
    source: &'a [u8],
    out: &mut Vec<(String, tree_sitter::Node<'a>)>,
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
        out.push((name.to_string(), member));
    }
}

fn check_decl(
    node: tree_sitter::Node,
    type_name: &str,
    body: tree_sitter::Node,
    source: &[u8],
    ctx_path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut props = Vec::new();
    collect_prop_names(body, source, &mut props);

    let server_fields: Vec<&str> = props
        .iter()
        .filter(|(n, _)| SERVER_MANAGED_FIELDS.contains(&n.as_str()))
        .map(|(n, _)| n.as_str())
        .collect();

    if server_fields.is_empty() {
        return;
    }

    // Only flag types whose name signals input use, OR bare entity names
    // (no output suffix) that embed server-managed fields.
    let is_input_named = has_input_suffix(type_name);
    let is_bare_entity = !has_output_suffix(type_name) && !has_input_suffix(type_name);

    if !is_input_named && !is_bare_entity {
        return;
    }

    let joined = server_fields.join(", ");
    diagnostics.push(Diagnostic::at_node(
        ctx_path,
        &node,
        super::META.id,
        format!(
            "Type `{type_name}` mixes server-managed fields ({joined}) with other fields; split into separate input/output types so clients don't own server-assigned values."
        ),
        Severity::Warning,
    ));
}

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        "interface_declaration" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else { return };
            let Some(body) = node.child_by_field_name("body") else { return };
            check_decl(node, name, body, source, ctx.path, diagnostics);
        }
        "type_alias_declaration" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else { return };
            let Some(value) = node.child_by_field_name("value") else { return };
            if value.kind() != "object_type" { return }
            check_decl(node, name, value, source, ctx.path, diagnostics);
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
    fn flags_input_type_with_server_fields() {
        let d = run("interface CreateUserInput { id: string; name: string; createdAt: string }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("CreateUserInput"));
    }

    #[test]
    fn flags_bare_entity_with_server_fields() {
        let d = run("interface User { id: string; name: string; createdAt: string }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_type_alias_request_with_server_fields() {
        let d = run("type UpdateOrderRequest = { id: string; total: number; updatedAt: string };");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_response_type_with_server_fields() {
        assert!(run("interface UserResponse { id: string; name: string; createdAt: string }").is_empty());
    }

    #[test]
    fn allows_input_without_server_fields() {
        assert!(run("interface CreateUserInput { name: string; email: string }").is_empty());
    }
}

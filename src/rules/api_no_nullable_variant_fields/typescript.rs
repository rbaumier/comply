//! Collect optional property names in an interface / object type. If
//! two or more share a common alphabetic prefix (≥ 4 chars, e.g.
//! `cancel…`, `shipped…`), flag the declaration: this is the
//! "conditional optional fields" smell. Two related optional fields
//! (e.g. `cancelReason?` + `cancelledAt?`) are already enough to
//! warrant a discriminated union.

use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::FxHashMap;

fn is_optional_property(member: tree_sitter::Node) -> bool {
    // tree-sitter-typescript marks optional properties either with
    // `optional: "?"` child or by the prop kind directly. Walk children
    // looking for a literal "?" token.
    let mut cursor = member.walk();
    for child in member.children(&mut cursor) {
        if child.kind() == "?" {
            return true;
        }
    }
    false
}

/// True when the property's type annotation is the `never` keyword.
/// Used to skip phantom / mutually-exclusive-props patterns such as
/// `{ page?: never; pageSize?: never }`, which are type-level
/// exclusions, not state-variant clusters.
fn has_never_annotation(member: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(annotation) = member.child_by_field_name("type") else {
        return false;
    };
    // annotation is `type_annotation` wrapping the actual type node.
    let mut cursor = annotation.walk();
    for child in annotation.children(&mut cursor) {
        if child.kind() == "predefined_type"
            && std::str::from_utf8(&source[child.byte_range()]).unwrap_or("") == "never"
        {
            return true;
        }
    }
    false
}

/// Return a 4-character lowercase prefix bucket for `name`, so close
/// variants such as `cancelReason` and `cancelledAt` collide on the
/// same bucket (`canc`). Returns the empty string when the name has
/// fewer than 4 leading ASCII alphabetic characters.
fn leading_prefix(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut buf = String::with_capacity(4);
    for &b in bytes.iter().take(4) {
        if !b.is_ascii_alphabetic() {
            return String::new();
        }
        buf.push(b.to_ascii_lowercase() as char);
    }
    if buf.len() < 4 {
        return String::new();
    }
    buf
}

fn is_inside_ambient_declaration(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "ambient_declaration" {
            return true;
        }
        cur = n.parent();
    }
    false
}

fn collect_optional_prefixes(body: tree_sitter::Node, source: &[u8]) -> FxHashMap<String, usize> {
    let mut counts: FxHashMap<String, usize> = FxHashMap::default();
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() != "property_signature" {
            continue;
        }
        if !is_optional_property(member) {
            continue;
        }
        if has_never_annotation(member, source) {
            continue;
        }
        let Some(name_node) = member.child_by_field_name("name") else {
            continue;
        };
        let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else {
            continue;
        };
        let prefix = leading_prefix(name);
        if prefix.len() < 4 {
            continue;
        }
        *counts.entry(prefix).or_insert(0) += 1;
    }
    counts
}

fn check_decl(
    node: tree_sitter::Node,
    type_name: &str,
    body: tree_sitter::Node,
    source: &[u8],
    ctx_path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let counts = collect_optional_prefixes(body, source);
    let mut hits: Vec<(&String, &usize)> = counts.iter().filter(|(_, c)| **c >= 2).collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_by(|a, b| b.1.cmp(a.1));
    let (prefix, count) = hits[0];
    diagnostics.push(Diagnostic::at_node(
        ctx_path,
        &node,
        super::META.id,
        format!(
            "`{type_name}` has {count} optional fields sharing prefix `{prefix}…` — encode this state with a discriminated union instead."
        ),
        Severity::Warning,
    ));
}

crate::ast_check! { on ["interface_declaration", "type_alias_declaration"] => |node, source, ctx, diagnostics|
    if is_inside_ambient_declaration(node) { return; }
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
    fn flags_three_optional_fields_sharing_prefix() {
        let d = run(
            "interface Order { id: string; cancelReason?: string; cancelNote?: string; cancelCode?: string }",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("canc"));
    }

    #[test]
    fn flags_two_optional_fields_sharing_prefix() {
        // REVIEW regression: two related optional fields are enough —
        // `status: "cancelled"` + `cancelReason?` + `cancelledAt?` is the
        // canonical conditional-fields smell.
        let d = run("interface Order { id: string; cancelReason?: string; cancelledAt?: string }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("canc"));
    }

    #[test]
    fn flags_prefix_in_type_alias() {
        let d = run(
            "type Shipment = { id: string; shippedAt?: string; shippedBy?: string; shippedVia?: string };",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_optional_fields_without_shared_prefix() {
        assert!(
            run("interface User { id: string; name?: string; email?: string; phone?: string }")
                .is_empty()
        );
    }

    #[test]
    fn allows_required_fields_sharing_prefix() {
        assert!(
            run(
                "interface Order { cancelReason: string; cancelledAt: string; cancelledBy: string }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_single_optional_field() {
        assert!(run("interface Order { id: string; cancelReason?: string }").is_empty());
    }

    #[test]
    fn allows_phantom_never_props() {
        // Regression for #120: `{ page?: never; pageSize?: never; q?: never; sort?: never }`
        // is a mutually-exclusive-props / phantom-key pattern, not a
        // state-variant cluster. The optional `?: never` declares the
        // key MUST be absent — opposite of an optional state flag.
        assert!(
            run(
                "type Phantom = { page?: never; pageSize?: never; q?: never; sort?: never };"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_declare_module_augmentation() {
        // Regression for #544: module augmentations are not API response types;
        // optional fields are intentional route metadata, not state-variant clusters.
        assert!(
            run(
                "declare module '@tanstack/react-router' {\
  interface StaticDataRouteOption {\
    title?: string;\
    breadcrumbParent?: string;\
    breadcrumbAncestors?: { title: string; pathname: string }[];\
  }\
}"
            )
            .is_empty()
        );
    }
}

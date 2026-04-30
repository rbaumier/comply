//! jsx-fragments backend — flag `<React.Fragment>` or bare `<Fragment>`
//! opening elements, except when a `key` prop forces the long form.

use crate::diagnostic::{Diagnostic, Severity};

/// True if `name_node` resolves to the `Fragment` tag — either the
/// bare identifier `Fragment` or `React.Fragment`.
fn is_fragment_tag(name_node: tree_sitter::Node, source: &[u8]) -> bool {
    match name_node.kind() {
        "identifier" | "jsx_identifier" => &source[name_node.byte_range()] == b"Fragment",
        "member_expression" | "nested_identifier" => {
            let Some(object) = name_node
                .child_by_field_name("object")
                .or_else(|| name_node.child(0))
            else {
                return false;
            };
            let Some(property) = name_node
                .child_by_field_name("property")
                .or_else(|| name_node.child(name_node.child_count().saturating_sub(1)))
            else {
                return false;
            };
            &source[object.byte_range()] == b"React"
                && &source[property.byte_range()] == b"Fragment"
        }
        _ => false,
    }
}

/// True if the opening element has a `key` attribute.
fn has_key_attribute(opening: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = opening.walk();
    for child in opening.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        if let Some(name) = crate::rules::jsx::jsx_attribute_name(child, source)
            && name == "key"
        {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["jsx_opening_element"] prefilter = ["Fragment"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    if !is_fragment_tag(name_node, source) {
        return;
    }
    if has_key_attribute(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "jsx-fragments".into(),
        message: "Prefer the short fragment syntax `<>...</>`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_react_fragment() {
        let d = run_on("const x = <React.Fragment><Child /></React.Fragment>;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bare_fragment() {
        let d = run_on("const x = <Fragment><Child /></Fragment>;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_short_fragment() {
        assert!(run_on("const x = <><Child /></>;").is_empty());
    }

    #[test]
    fn allows_react_fragment_with_key() {
        let src = "const x = <React.Fragment key={id}><Child /></React.Fragment>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_fragment_with_key() {
        let src = "const x = <Fragment key={id}><Child /></Fragment>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_regular_component() {
        assert!(run_on("const x = <Foo><Child /></Foo>;").is_empty());
    }
}

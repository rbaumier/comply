//! AST backend for react-no-find-in-map-loop.
//!
//! Fires on a `.find(...)` / `.filter(...)` call when one of its
//! enclosing ancestors is:
//! - a `.map(...)` callback, or
//! - a `for` / `for_in` / `for_of` / `while` loop body.

use crate::diagnostic::{Diagnostic, Severity};

fn is_member_call(call: tree_sitter::Node<'_>, method: &str, source: &[u8]) -> bool {
    let Some(callee) = call.child_by_field_name("function") else { return false };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return false };
    prop.utf8_text(source).ok() == Some(method)
}

fn inside_loop_or_map(mut node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "for_statement" | "for_in_statement" | "while_statement" | "do_statement" => {
                return true;
            }
            "call_expression" => {
                if is_member_call(parent, "map", source) {
                    return true;
                }
            }
            _ => {}
        }
        node = parent;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    if node.kind() != "call_expression" {
        return;
    }
    if !is_member_call(node, "find", source) && !is_member_call(node, "filter", source) {
        return;
    }
    if !inside_loop_or_map(node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`.find`/`.filter` inside a `.map` or loop — O(n²). \
         Build a `Map` once and look up inside the loop."
            .into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_find_inside_map() {
        let src = r#"items.map(i => others.find(o => o.id === i.id));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_filter_inside_for() {
        let src = r#"
for (const i of items) {
  const matches = others.filter(o => o.id === i.id);
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_find_at_top_level() {
        let src = r#"const x = items.find(i => i.id === 1);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_without_nested_find() {
        let src = r#"items.map(i => i.id);"#;
        assert!(run(src).is_empty());
    }
}

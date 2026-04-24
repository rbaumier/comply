//! Detection: a `generic_type` node whose outer type is `Arc` (or a
//! path ending in `::Arc`) wrapping a `generic_type` on
//! `Mutex`/`RwLock`, or `Rc` wrapping `RefCell`/`Cell`. We then ask
//! whether the innermost type looks like a tree/graph node — the
//! name ends with `Node`, or contains `Tree`/`Graph`, or matches a
//! small set of common aliases (`Link`, `Edge`, `Vertex`).
//!
//! Arbitrary `Arc<Mutex<Foo>>` for non-node data is a perfectly
//! valid pattern (shared state, task handles), so we gate on the
//! innermost name to keep the rule focused on tree/graph shapes.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "generic_type" { return; }

    let Some(outer_name) = type_name(node, source) else { return; };
    let outer_is_shared_ptr = matches!(outer_name, "Arc" | "Rc")
        || outer_name.ends_with("::Arc")
        || outer_name.ends_with("::Rc");
    if !outer_is_shared_ptr { return; }

    let Some(first_arg) = first_type_arg(node) else { return; };
    if first_arg.kind() != "generic_type" { return; }

    let Some(mid_name) = type_name(first_arg, source) else { return; };
    let outer_is_arc = outer_name == "Arc" || outer_name.ends_with("::Arc");
    let valid_inner = if outer_is_arc {
        matches!(mid_name, "Mutex" | "RwLock")
            || mid_name.ends_with("::Mutex")
            || mid_name.ends_with("::RwLock")
    } else {
        matches!(mid_name, "RefCell" | "Cell")
            || mid_name.ends_with("::RefCell")
            || mid_name.ends_with("::Cell")
    };
    if !valid_inner { return; }

    let Some(inner_arg) = first_type_arg(first_arg) else { return; };
    let inner_text = match inner_arg.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    if !looks_like_node_type(inner_text) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{outer_name}<{mid_name}<{inner_text}>>` for tree/graph nodes is \
             slow and cycle-unfriendly. Consider an arena (`id_arena`, \
             `indextree`, `slotmap`) where nodes are indices into a single \
             `Vec<T>`."
        ),
        Severity::Warning,
    ));
}

fn type_name<'a>(generic: Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let type_node = generic.child_by_field_name("type")?;
    type_node.utf8_text(source).ok()
}

fn first_type_arg<'a>(generic: Node<'a>) -> Option<Node<'a>> {
    let args = generic.child_by_field_name("type_arguments")?;
    let mut cursor = args.walk();
    args.named_children(&mut cursor)
        .find(|c| c.kind() != "type_binding" && c.kind() != "lifetime")
}

fn looks_like_node_type(name: &str) -> bool {
    // Strip any leading path segments — `self::Node` → `Node`.
    let leaf = name.rsplit("::").next().unwrap_or(name);
    // Strip generics / references / lifetimes: take the leading
    // identifier-ish prefix.
    let ident: String = leaf
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let ident_lc = ident.to_ascii_lowercase();
    ident_lc.ends_with("node")
        || ident_lc.ends_with("tree")
        || ident_lc.contains("graph")
        || matches!(ident_lc.as_str(), "link" | "edge" | "vertex")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_arc_mutex_node() {
        let src = "struct T { children: Vec<Arc<Mutex<Node>>> }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_rc_refcell_tree_node() {
        let src = "struct T { root: Rc<RefCell<TreeNode>> }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_arc_mutex_on_non_node_state() {
        let src = "struct State { handle: Arc<Mutex<DbPool>> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_plain_arc_of_node() {
        // No interior mutability layer → this rule doesn't fire.
        let src = "struct T { child: Arc<Node> }";
        assert!(run(src).is_empty());
    }
}

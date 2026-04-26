//! no-unused-collection AST backend — collection populated but never read.
//!
//! Walks `variable_declarator` nodes whose initializer is an array literal
//! or a `new Map/Set/Array/WeakMap/WeakSet(...)` expression. For each
//! candidate, scans the surrounding tree for identifier usages and
//! classifies each as a write (mutation method call) or a read (anything
//! else). Flags when a collection is written but never read.

use crate::diagnostic::{Diagnostic, Severity};

/// Mutation methods — when called on the collection identifier, count as a write.
const WRITE_METHODS: &[&str] = &["push", "add", "set", "unshift", "splice"];

/// Returns the constructor name for `new <Name>(...)`, or `None`.
fn new_expression_ctor<'a>(value: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if value.kind() != "new_expression" {
        return None;
    }
    let ctor = value.child_by_field_name("constructor")?;
    ctor.utf8_text(source).ok()
}

/// True if `value` is a recognised collection initializer.
fn is_collection_initializer(value: tree_sitter::Node, source: &[u8]) -> bool {
    if value.kind() == "array" {
        return true;
    }
    matches!(
        new_expression_ctor(value, source),
        Some("Map" | "Set" | "Array" | "WeakMap" | "WeakSet")
    )
}

/// True when `id_node` is the `object` part of `<id>.<method>(...)` and
/// `<method>` is a known mutation method.
fn is_write_usage(id_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = id_node.parent() else { return false };
    if parent.kind() != "member_expression" {
        return false;
    }
    // Identifier must be the *object*, not the property — otherwise `foo.push`
    // accessing a property named the same as our collection would be misread.
    let Some(obj) = parent.child_by_field_name("object") else { return false };
    if obj.id() != id_node.id() {
        return false;
    }
    let Some(grand) = parent.parent() else { return false };
    if grand.kind() != "call_expression" {
        return false;
    }
    // Ensure the member_expression is the *callee*, not an argument.
    let Some(callee) = grand.child_by_field_name("function") else { return false };
    if callee.id() != parent.id() {
        return false;
    }
    let Some(prop) = parent.child_by_field_name("property") else { return false };
    let method = prop.utf8_text(source).unwrap_or("");
    WRITE_METHODS.contains(&method)
}

/// Walk the whole tree under `root` and return (`is_written`, `is_read`)
/// for `name`, ignoring the declarator node itself.
fn classify_usages(
    root: tree_sitter::Node,
    decl_id_byte: usize,
    name: &str,
    source: &[u8],
) -> (bool, bool) {
    let mut is_written = false;
    let mut is_read = false;
    let mut cursor = root.walk();
    let mut stack: Vec<tree_sitter::Node> = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" && n.utf8_text(source).unwrap_or("") == name {
            // Skip the declarator's own name node.
            if n.start_byte() != decl_id_byte {
                if is_write_usage(n, source) {
                    is_written = true;
                } else {
                    is_read = true;
                }
            }
        }
        for child in n.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    (is_written, is_read)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "variable_declarator" {
        return;
    }

    // Only `const x = ...` declarations.
    let Some(decl) = node.parent() else { return };
    if decl.kind() != "lexical_declaration" {
        return;
    }
    let kind_text = decl.child(0)
        .and_then(|c| c.utf8_text(source).ok())
        .unwrap_or("");
    if kind_text != "const" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    if name_node.kind() != "identifier" {
        return;
    }
    let Ok(name) = name_node.utf8_text(source) else { return };

    let Some(value) = node.child_by_field_name("value") else { return };
    if !is_collection_initializer(value, source) {
        return;
    }

    // Scan from the program root so that usages anywhere in the file count.
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }

    let (is_written, is_read) =
        classify_usages(root, name_node.start_byte(), name, source);

    if is_written && !is_read {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &name_node,
            "no-unused-collection",
            format!("Collection `{name}` is populated but never read."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_pushed_but_never_read() {
        let src = r#"
const items = [];
items.push(1);
items.push(2);
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_set_add_but_never_read() {
        let src = r#"
const seen = new Set();
seen.add("a");
seen.add("b");
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_pushed_and_iterated() {
        let src = r#"
const items = [];
items.push(1);
items.forEach(x => console.log(x));
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pushed_and_returned() {
        let src = r#"
const items = [];
items.push(1);
return items;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_collection_passed_as_arg() {
        let src = r#"
const items = [];
items.push(1);
doSomething(items);
"#;
        assert!(run_on(src).is_empty());
    }
}

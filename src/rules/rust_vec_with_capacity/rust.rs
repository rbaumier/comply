//! rust-vec-with-capacity backend.
//!
//! Matches `let [mut] X = Vec::new()` declarations and checks whether a
//! following sibling `for_expression` pushes into `X`. When both are
//! present, the Vec's final length is knowable up front and
//! `Vec::with_capacity(n)` avoids the log2(n) reallocation chain from
//! doubling.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "let_declaration" { return; }

    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "call_expression" { return; }
    let Some(fn_node) = value.child_by_field_name("function") else { return; };
    let fn_text = fn_node.utf8_text(source).unwrap_or("");
    if fn_text != "Vec::new" && fn_text != "std::vec::Vec::new" { return; }

    let Some(pattern) = node.child_by_field_name("pattern") else { return; };
    let Some(var_name) = extract_var_name(pattern, source) else { return; };

    let Some(parent) = node.parent() else { return; };
    let mut cursor = parent.walk();
    let mut after_us = false;
    let mut has_for_with_push = false;
    for sib in parent.children(&mut cursor) {
        if sib.id() == node.id() {
            after_us = true;
            continue;
        }
        if !after_us { continue; }
        let for_node = if sib.kind() == "for_expression" {
            sib
        } else if sib.kind() == "expression_statement"
            && let Some(inner) = sib.named_child(0)
            && inner.kind() == "for_expression"
        {
            inner
        } else {
            continue;
        };
        if let Some(body) = for_node.child_by_field_name("body")
            && contains_push(body, var_name, source)
        {
            has_for_with_push = true;
            break;
        }
    }

    if has_for_with_push {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &value,
            super::META.id,
            format!("Use `Vec::with_capacity(...)` instead of `Vec::new()` when `{var_name}` is populated in a for-loop."),
            Severity::Warning,
        ));
    }
}

fn extract_var_name<'a>(pattern: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if pattern.kind() == "identifier" {
        return pattern.utf8_text(source).ok();
    }
    if pattern.kind() == "mut_pattern" {
        let mut cursor = pattern.walk();
        for child in pattern.children(&mut cursor) {
            if child.kind() == "identifier" {
                return child.utf8_text(source).ok();
            }
        }
    }
    None
}

fn contains_push(node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    if node.kind() == "call_expression"
        && let Some(fn_node) = node.child_by_field_name("function")
        && fn_node.kind() == "field_expression"
    {
        let val = fn_node
            .child_by_field_name("value")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        let field = fn_node
            .child_by_field_name("field")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if val == var && field == "push" {
            return true;
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_push(child, var, source) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_vec_new_then_push_in_for() {
        let src = "fn f(items: Vec<i32>) {\n    let mut result = Vec::new();\n    for item in items {\n        result.push(item);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_capacity() {
        let src = "fn f(items: Vec<i32>) {\n    let mut result = Vec::with_capacity(items.len());\n    for item in items {\n        result.push(item);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_vec_new_no_for() {
        assert!(run("fn f() {\n    let mut v = Vec::new();\n    v.push(1);\n}").is_empty());
    }
}

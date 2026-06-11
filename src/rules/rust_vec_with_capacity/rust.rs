//! rust-vec-with-capacity backend.
//!
//! Matches `let [mut] X = Vec::new()` declarations and checks whether a
//! following sibling `for_expression` pushes into `X` unconditionally:
//! the `X.push(...)` must be a direct statement of the loop body (not
//! nested inside an `if`/`match`) and the body must contain no `continue`
//! that would skip iterations. Only then does the Vec's final length equal
//! the iterable's length, making `Vec::with_capacity(n)` the right call —
//! it avoids the log2(n) reallocation chain from doubling. A conditional
//! push or a `continue` makes the final length unknowable up front, so
//! `with_capacity` would mis-size.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["let_declaration"] => |node, source, ctx, diagnostics|
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
            && body_directly_pushes(body, var_name, source)
            && !body_has_continue(body)
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

fn is_push_call(node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
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
        return val == var && field == "push";
    }
    false
}

fn body_directly_pushes(body: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        let call = if child.kind() == "call_expression" {
            child
        } else if child.kind() == "expression_statement" {
            match child.named_child(0) {
                Some(inner) if inner.kind() == "call_expression" => inner,
                _ => continue,
            }
        } else {
            continue;
        };
        if is_push_call(call, var, source) {
            return true;
        }
    }
    false
}

fn body_has_continue(node: tree_sitter::Node) -> bool {
    if node.kind() == "continue_expression" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(body_has_continue)
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
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

    #[test]
    fn allows_conditional_push_in_if_issue_1024() {
        let src = "fn f(items: Vec<i32>) {\n    let mut v = Vec::new();\n    for x in items {\n        if x > 0 { v.push(x); }\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_push_with_continue_in_body_issue_1024() {
        let src = "fn f(items: Vec<i32>) {\n    let mut ok = Vec::new();\n    for x in items {\n        if x < 0 { continue; }\n        ok.push(x);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_push_nested_in_double_if_issue_1024() {
        let src = "fn f(items: Vec<Option<i32>>) {\n    let mut names = Vec::new();\n    for x in items {\n        if true {\n            if let Some(v) = x {\n                names.push(v);\n            }\n        }\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_unconditional_push_with_unrelated_if() {
        let src = "fn f(items: Vec<i32>) {\n    let mut out = Vec::new();\n    for x in items {\n        if x > 0 { println!(\"{x}\"); }\n        out.push(x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }
}

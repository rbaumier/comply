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
//!
//! The iterable itself must be length-bearing — a bare binding or field of a
//! collection type (`v`, `self.items`), optionally behind one reference
//! (`&v`). Every other iterable shape is skipped: lazy/fallible ones in
//! particular (`make_items()`, `Iter::new(r)?`, `v.iter().filter(..)`) have no
//! cheaply known length to size the capacity from, so `with_capacity(n)` can't
//! be written.

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
        if iterable_has_known_length(for_node)
            && let Some(body) = for_node.child_by_field_name("body")
            && body_directly_pushes(body, var_name, source)
            && !body_has_continue(body)
            && !body_extends_or_appends(body, var_name, source)
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

/// Whether the `for_expression`'s iterable is a value whose length is cheaply
/// known, so `Vec::with_capacity(n)` has an `n` to supply. Length-bearing means
/// a bare `identifier` or `field_expression` (`v`, `self.items`), optionally
/// behind a single `reference_expression` (`&v`). Every other shape is skipped,
/// notably the lazy/fallible iterators that have no cheaply available length: a
/// `call_expression` (`make_items()`), a `try_expression` (`Iter::new(r)?`), or
/// an iterator-adaptor chain (`v.iter().filter(..)`, parsed as a
/// `call_expression` whose function is a `field_expression`).
fn iterable_has_known_length(for_node: tree_sitter::Node) -> bool {
    let Some(value) = for_node.child_by_field_name("value") else { return false; };
    let inner = if value.kind() == "reference_expression" {
        match value.child_by_field_name("value") {
            Some(n) => n,
            None => return false,
        }
    } else {
        value
    };
    matches!(inner.kind(), "identifier" | "field_expression")
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

/// Whether `node` is a `<var>.extend(...)` or `<var>.append(...)` call. Both add
/// a statically-unknown number of elements, so the Vec's final length stops
/// equalling the iteration count and `with_capacity(n)` would under-allocate.
fn is_extend_or_append_call(node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
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
        return val == var && (field == "extend" || field == "append");
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

/// Whether the loop body contains any `<var>.extend(...)`/`<var>.append(...)`
/// anywhere — including nested inside an `if`/`if let` — using the same
/// whole-subtree walk as `body_has_continue` so a conditional extend is caught.
fn body_extends_or_appends(node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    if is_extend_or_append_call(node, var, source) {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| body_extends_or_appends(child, var, source))
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

    #[test]
    fn skips_fallible_iterator_iterable_issue_3983() {
        let src = "fn read<'a>(r: &mut Reader<'a>) -> Result<Vec<CertificateDer<'a>>, InvalidMessage> {\n    let mut ret = Vec::new();\n    for item in TlsListIter::<CertificateDer<'a>>::new(r)? {\n        ret.push(item?);\n    }\n    Ok(ret)\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_iterator_adaptor_chain_iterable() {
        let src = "fn f(v: Vec<i32>) {\n    let mut out = Vec::new();\n    for x in v.iter().filter(|x| **x > 0) {\n        out.push(*x);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_plain_call_iterable() {
        let src = "fn f() {\n    let mut out = Vec::new();\n    for x in make_items() {\n        out.push(x);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_field_iterable() {
        let src = "struct S { items: Vec<i32> }\nimpl S {\n    fn f(&self) {\n        let mut out = Vec::new();\n        for x in self.items {\n            out.push(x);\n        }\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_reference_iterable() {
        let src = "fn f(v: &Vec<i32>) {\n    let mut out = Vec::new();\n    for x in &v {\n        out.push(*x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_push_with_extend_same_vec_issue_3947() {
        let src = "fn f(xs: Vec<i32>, other: Vec<i32>) {\n    let mut v = Vec::new();\n    for x in xs {\n        v.push(x);\n        v.extend(other.clone());\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_push_with_append_same_vec_issue_3947() {
        let src = "fn f(xs: Vec<i32>) {\n    let mut v = Vec::new();\n    let mut more = vec![1];\n    for x in xs {\n        v.push(x);\n        v.append(&mut more);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_push_with_conditional_extend_same_vec_issue_3947() {
        let src = "fn f(summaries: Vec<S>) {\n    let mut ids = Vec::new();\n    for summary in summaries {\n        ids.push(summary.package_id());\n        if let Some(lock) = summary.lock {\n            ids.extend(lock.alt);\n        }\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_extend_on_different_var_issue_3947() {
        let src = "fn f(xs: Vec<i32>, z: Vec<i32>) {\n    let mut v = Vec::new();\n    let mut other = Vec::new();\n    for x in xs {\n        v.push(x);\n        other.extend(z.clone());\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }
}

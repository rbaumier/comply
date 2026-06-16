//! no-for-loop Rust backend.
//!
//! Flag `while` loops with manual index that could be `for item in iter`.
//! Rust doesn't have C-style `for` loops, but `while i < len { ... i += 1 }`
//! is the equivalent anti-pattern.
//!
//! Index loops that remove elements from the indexed collection during
//! traversal (`vec.remove(i)` / `vec.swap_remove(i)`) are exempt: removal
//! shifts the remaining elements, so a `for`/iterator rewrite is impossible.
//!
//! Two-pointer loops are exempt: when the body mutates two or more distinct
//! index variables (`old_idx += 1`, `new_idx += 1`), the indices advance at
//! different rates and no single iterator combinator expresses the traversal.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

crate::ast_check! { on ["while_expression"] => |node, source, ctx, diagnostics|
    let Some(condition) = node.child_by_field_name("condition") else { return };
    let Ok(cond_text) = condition.utf8_text(source) else { return };

    // Heuristic: `i < something.len()` or `i < N`.
    if !cond_text.contains(".len()") && !cond_text.contains("< ") {
        return;
    }

    // Check the body for `i += 1` pattern.
    let Some(body) = node.child_by_field_name("body") else { return };
    let Ok(body_text) = body.utf8_text(source) else { return };

    if !body_text.contains("+= 1") && !body_text.contains("= i + 1") {
        return;
    }

    // Exempt loops in a const-evaluated context (`const`/`static` initializer
    // or `const fn` body): `for` desugars to `IntoIterator::into_iter`, which
    // is not `const`, so a manual index loop is the only valid iteration there.
    if crate::rules::rust_helpers::is_in_const_eval_context(node, source) {
        return;
    }

    // Exempt loops that remove elements from the indexed collection during
    // traversal: `remove(i)`/`swap_remove(i)` shifts the remaining elements,
    // so the index is advanced conditionally and no `for`/iterator rewrite is
    // possible. The argument must be the loop's index variable — a `remove`
    // on an unrelated collection (e.g. `map.remove(&key)`) does not exempt.
    if let Some(index_var) = index_variable(condition, source)
        && body_removes_at_index(body, index_var, source)
    {
        return;
    }

    // Exempt two-pointer loops: when two or more distinct index variables are
    // mutated inside the body (`old_idx += 1`, `new_idx += 1`), the indices
    // advance at different rates and the traversal cannot be expressed as a
    // single `for`/iterator rewrite.
    if count_mutated_index_variables(body, source) >= 2 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-for-loop".into(),
        message: "Manual index loop — use `for item in collection` or `.iter().enumerate()`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// Extract the loop's index variable from the condition: the bare identifier
/// on the left of a `<` comparison whose right side is a `.len()` call
/// (`i < self.items.len()`). Returns its source text.
fn index_variable<'a>(condition: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut stack = vec![condition];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "binary_expression"
            && let Some(op) = cur.child_by_field_name("operator")
            && op.utf8_text(source) == Ok("<")
            && let Some(left) = cur.child_by_field_name("left")
            && left.kind() == "identifier"
            && let Some(right) = cur.child_by_field_name("right")
            && subtree_calls_len(right, source)
            && let Ok(name) = left.utf8_text(source)
        {
            return Some(name);
        }
        let mut cursor = cur.walk();
        for child in cur.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

/// Count the distinct bare-identifier index variables mutated anywhere in the
/// loop body. A mutation is a `compound_assignment_expr` (`x += k`) or an
/// `assignment_expression` (`x = …`) whose left-hand side is a plain
/// identifier. Property/element/deref targets (`*sum += …`, `self.n += 1`,
/// `v[i] = …`) are not bare identifiers and do not count, so a single-index
/// loop accumulating into `*sum` still reports one mutated index.
fn count_mutated_index_variables(body: tree_sitter::Node, source: &[u8]) -> usize {
    let mut vars: HashSet<&str> = HashSet::new();
    let mut stack = vec![body];
    while let Some(cur) = stack.pop() {
        if matches!(cur.kind(), "compound_assignment_expr" | "assignment_expression")
            && let Some(left) = cur.child_by_field_name("left")
            && left.kind() == "identifier"
            && let Ok(name) = left.utf8_text(source)
        {
            vars.insert(name);
        }
        let mut cursor = cur.walk();
        for child in cur.children(&mut cursor) {
            stack.push(child);
        }
    }
    vars.len()
}

/// True if `node` contains a `.len()` method call anywhere in its subtree.
fn subtree_calls_len(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut stack = vec![node];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "call_expression"
            && let Some(func) = cur.child_by_field_name("function")
            && func.kind() == "field_expression"
            && let Some(field) = func.child_by_field_name("field")
            && field.utf8_text(source) == Ok("len")
        {
            return true;
        }
        let mut cursor = cur.walk();
        for child in cur.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if the loop body contains a `remove(index_var)` or
/// `swap_remove(index_var)` method call whose sole argument is the loop's
/// index variable as a bare identifier. Nested loops are not skipped: a
/// removal anywhere inside the body is enough to make the rewrite impossible.
fn body_removes_at_index(body: tree_sitter::Node, index_var: &str, source: &[u8]) -> bool {
    let mut stack = vec![body];
    while let Some(cur) = stack.pop() {
        if is_remove_at_index(cur, index_var, source) {
            return true;
        }
        let mut cursor = cur.walk();
        for child in cur.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if `node` is `<receiver>.remove(index_var)` or
/// `<receiver>.swap_remove(index_var)`.
fn is_remove_at_index(node: tree_sitter::Node, index_var: &str, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    let Some(field) = func.child_by_field_name("field") else {
        return false;
    };
    let method = field.utf8_text(source).unwrap_or("");
    if method != "remove" && method != "swap_remove" {
        return false;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        if arg.kind() == "identifier" && arg.utf8_text(source) == Ok(index_var) {
            return true;
        }
    }
    false
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
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_manual_index_loop() {
        let src = "fn f(v: &[i32]) { let mut i = 0; while i < v.len() { println!(\"{}\", v[i]); i += 1; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_for_in() {
        let src = "fn f(v: &[i32]) { for item in v { println!(\"{item}\"); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_remove_during_traversal_issue_1509() {
        // typst pattern from the issue: conditional `remove(i)` during traversal.
        let src = "fn f(s: &mut S, list: &mut Vec<Item>, n: usize) { \
                   let mut i = 0; \
                   while i < s.items.len() && list.len() < n { \
                   if s.items[i].name.is_none() { list.push(s.items.remove(i)); } \
                   else { i += 1; } \
                   } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_minimal_remove_during_traversal() {
        let src = "fn f(v: &mut Vec<i32>) { \
                   let mut i = 0; \
                   while i < v.len() { \
                   if pred(&v[i]) { v.remove(i); } else { i += 1; } \
                   } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_swap_remove_during_traversal() {
        let src = "fn f(v: &mut Vec<i32>) { \
                   let mut i = 0; \
                   while i < v.len() { \
                   if pred(&v[i]) { v.swap_remove(i); } else { i += 1; } \
                   } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_read_only_index_loop() {
        // No removal: a plain read-only index loop still fires.
        let src = "fn f(v: &[i32], sum: &mut i32) { \
                   let mut i = 0; \
                   while i < v.len() { *sum += v[i]; i += 1; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_two_pointer_loop_issue_1520() {
        // actix-web normalize.rs: `old_idx` advances every step, `new_idx` only
        // on match — two distinct mutated index variables, no iterator rewrite.
        let src = "fn f(old_path: &[u8], new_path: &[u8], map: &mut Vec<u16>) { \
                   let mut old_idx = 0usize; let mut new_idx = 0usize; \
                   while old_idx < old_path.len() { \
                   if new_idx < new_path.len() && old_path[old_idx] == new_path[new_idx] { new_idx += 1; } \
                   old_idx += 1; \
                   map.push(new_idx.min(u16::MAX as usize) as u16); \
                   } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_single_index_loop_with_deref_accumulator() {
        // `*sum += v[i]` mutates a deref target, not a bare index — only `i` is
        // a mutated index variable, so a plain single-index loop still fires.
        let src = "fn f(v: &[i32], sum: &mut i32) { \
                   let mut i = 0; \
                   while i < v.len() { *sum += v[i]; i += 1; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_while_index_loop_in_const_initializer_issue_3266() {
        // helix match_brackets.rs: a `for` loop is forbidden in const eval, so
        // the manual while-index loop is the only valid iteration.
        let src = "const PAIRS: [(char, char); 4] = { \
                   let mut pairs = [(' ', ' '); 4]; \
                   let mut idx = 0; \
                   while idx < 4 { pairs[idx] = (' ', ' '); idx += 1; } \
                   pairs };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_while_index_loop_in_static_initializer() {
        let src = "static TABLE: [u8; 4] = { \
                   let mut a = [0u8; 4]; \
                   let mut i = 0; \
                   while i < 4 { a[i] = i as u8; i += 1; } \
                   a };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_while_index_loop_in_const_fn() {
        let src = "const fn build() -> u8 { \
                   let mut i = 0; let mut s = 0; \
                   while i < 4 { s += i; i += 1; } \
                   s }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_while_index_loop_in_runtime_fn() {
        // A normal runtime `fn` re-enables the lint: the walk stops at its body.
        let src = "fn build(xs: &[i32]) { \
                   let mut i = 0; \
                   while i < xs.len() { use_it(xs[i]); i += 1; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_runtime_loop_in_module_alongside_const() {
        // The runtime loop is still flagged even when a `const` sits nearby:
        // the const-eval walk stops at the enclosing runtime `fn` boundary.
        let src = "mod m { \
                   const N: usize = 4; \
                   fn build(xs: &[i32]) { \
                   let mut i = 0; \
                   while i < xs.len() { use_it(xs[i]); i += 1; } } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_remove_on_unrelated_collection() {
        // `map.remove(&key)` does not use the index variable — still fires.
        let src = "fn f(v: &[i32], map: &mut std::collections::HashMap<i32, i32>, key: i32) { \
                   let mut i = 0; \
                   while i < v.len() { map.remove(&key); i += 1; } }";
        assert_eq!(run_on(src).len(), 1);
    }
}

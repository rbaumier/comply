//! testing-no-shared-state backend — detect program-level `let`/`var`
//! bindings that are assigned inside a `test(...)` / `it(...)` callback,
//! unless a `beforeEach` block exists that also assigns them.
//!
//! Why: mutable module-level state makes tests order-dependent. Running
//! a single test in isolation passes; running the whole file fails.
//! Either scope the state per test or reset it in `beforeEach`.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

/// Collect names bound by `let`/`var` at program scope.
fn collect_program_let_var(root: tree_sitter::Node, source: &[u8], out: &mut Vec<(String, tree_sitter::Range)>) {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        let is_let = child.kind() == "lexical_declaration"
            && child.child(0).and_then(|n| n.utf8_text(source).ok()) == Some("let");
        let is_var = child.kind() == "variable_declaration";
        if !(is_let || is_var) { continue; }
        let mut dc = child.walk();
        for declarator in child.named_children(&mut dc) {
            if declarator.kind() != "variable_declarator" { continue; }
            let Some(name_node) = declarator.child_by_field_name("name") else { continue; };
            if name_node.kind() == "identifier" {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                out.push((name, declarator.range()));
            }
        }
    }
}

/// Mutating array/collection methods. A `name.push(...)` call mutates the
/// shared binding even though tree-sitter sees no `assignment_expression`.
const MUTATING_METHODS: &[&str] = &[
    "push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "copyWithin",
    "set", "delete", "clear", "add",
];

/// Does `fn_node`'s body contain an assignment OR a mutation to any of `names`?
///
/// Detects three flavours of mutation:
///   1. `name = ...` (re-assignment)
///   2. `name.prop = ...` / `name[key] = ...` (property write)
///   3. `name.push(...)` / `name.set(...)` / etc. (mutating method call)
fn body_assigns_any(fn_node: tree_sitter::Node, source: &[u8], names: &HashSet<String>) -> HashSet<String> {
    let mut found = HashSet::new();
    let Some(body) = fn_node.child_by_field_name("body") else { return found; };
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if n.kind() == "assignment_expression"
            && let Some(lhs) = n.child_by_field_name("left")
        {
            // Case 1: bare identifier on the LHS → reassignment.
            if lhs.kind() == "identifier" {
                let t = lhs.utf8_text(source).unwrap_or("").to_string();
                if names.contains(&t) { found.insert(t); }
            }
            // Cases 2: `obj.prop = ...` / `obj[k] = ...` — the LHS is a
            // member/subscript expression rooted at the shared identifier.
            if matches!(lhs.kind(), "member_expression" | "subscript_expression")
                && let Some(obj) = lhs.child_by_field_name("object")
                && obj.kind() == "identifier"
            {
                let t = obj.utf8_text(source).unwrap_or("").to_string();
                if names.contains(&t) { found.insert(t); }
            }
        }

        // Case 3: mutating method call — `name.push(...)`, `name.set(...)`, etc.
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
            && func.kind() == "member_expression"
            && let Some(obj) = func.child_by_field_name("object")
            && obj.kind() == "identifier"
            && let Some(prop) = func.child_by_field_name("property")
        {
            let obj_name = obj.utf8_text(source).unwrap_or("").to_string();
            let prop_name = prop.utf8_text(source).unwrap_or("");
            if names.contains(&obj_name) && MUTATING_METHODS.contains(&prop_name) {
                found.insert(obj_name);
            }
        }

        let mut c = n.walk();
        for child in n.named_children(&mut c) { stack.push(child); }
    }
    found
}

/// Walk call_expression nodes at any depth; return list of (callee-name, callback-node).
fn collect_hook_calls<'a>(root: tree_sitter::Node<'a>, source: &[u8], want: &[&str]) -> Vec<(String, tree_sitter::Node<'a>)> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
                && func.kind() == "identifier" {
                    let name = func.utf8_text(source).unwrap_or("");
                    if want.contains(&name)
                        && let Some(args) = n.child_by_field_name("arguments") {
                            let mut ac = args.walk();
                            for arg in args.named_children(&mut ac) {
                                if matches!(arg.kind(), "arrow_function" | "function_expression" | "function") {
                                    out.push((name.to_string(), arg));
                                }
                            }
                        }
                }
        let mut c = n.walk();
        for child in n.named_children(&mut c) { stack.push(child); }
    }
    out
}

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(
        &self,
        ctx: &crate::rules::backend::CheckCtx,
        tree: &tree_sitter::Tree,
    ) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let root = tree.root_node();

        // Gather program-level let/var bindings.
        let mut bindings: Vec<(String, tree_sitter::Range)> = Vec::new();
        collect_program_let_var(root, source, &mut bindings);
        if bindings.is_empty() { return Vec::new(); }

        let names: HashSet<String> = bindings.iter().map(|(n, _)| n.clone()).collect();

        // Collect test/it callbacks and beforeEach callbacks.
        let test_cbs: Vec<_> = collect_hook_calls(root, source, &["test", "it"])
            .into_iter().map(|(_, cb)| cb).collect();
        let before_each_cbs: Vec<_> = collect_hook_calls(root, source, &["beforeEach"])
            .into_iter().map(|(_, cb)| cb).collect();

        // Names reassigned inside test/it callbacks.
        let mut mutated_in_tests: HashSet<String> = HashSet::new();
        for cb in &test_cbs {
            for n in body_assigns_any(*cb, source, &names) {
                mutated_in_tests.insert(n);
            }
        }
        if mutated_in_tests.is_empty() { return Vec::new(); }

        // Names reset in a beforeEach.
        let mut reset_in_before_each: HashSet<String> = HashSet::new();
        for cb in &before_each_cbs {
            for n in body_assigns_any(*cb, source, &names) {
                reset_in_before_each.insert(n);
            }
        }

        let mut diagnostics = Vec::new();
        for (name, range) in &bindings {
            if !mutated_in_tests.contains(name) { continue; }
            if reset_in_before_each.contains(name) { continue; }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: range.start_point.row + 1,
                column: range.start_point.column + 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Top-level '{name}' is mutated inside test() without being reset in beforeEach — tests become order-dependent."
                ),
                severity: Severity::Warning,
                span: Some((range.start_byte, range.end_byte - range.start_byte)),
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_let_reassigned_in_test_without_before_each() {
        let src = "let counter = 0;\n\
                   test('a', () => { counter = counter + 1; });\n\
                   test('b', () => { counter = counter + 1; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_let_reset_in_before_each() {
        let src = "let counter = 0;\n\
                   beforeEach(() => { counter = 0; });\n\
                   test('a', () => { counter = counter + 1; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_const_even_if_read_in_tests() {
        let src = "const fixture = { x: 1 };\n\
                   test('a', () => { expect(fixture.x).toBe(1); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_let_not_mutated_in_any_test() {
        let src = "let config = { env: 'test' };\n\
                   test('a', () => { expect(config.env).toBe('test'); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_var_reassigned_in_it() {
        let src = "var state = null;\n\
                   it('a', () => { state = { ok: true }; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_array_push_mutation() {
        let src = "let items = [];\n\
                   test('a', () => { items.push(1); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_set_mutation() {
        let src = "let cache = new Map();\n\
                   test('a', () => { cache.set('k', 1); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_property_assignment() {
        let src = "let state = { x: 0 };\n\
                   test('a', () => { state.x = 1; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_subscript_assignment() {
        let src = "let bag = {};\n\
                   test('a', () => { bag['k'] = 1; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_array_push_reset_in_before_each() {
        let src = "let items = [];\n\
                   beforeEach(() => { items = []; });\n\
                   test('a', () => { items.push(1); });";
        assert!(run(src).is_empty());
    }
}

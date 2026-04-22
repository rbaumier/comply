//! inconsistent-function-call backend.
//!
//! Collects every `function_declaration` in the file, then scans all call
//! sites for each name. If a function is called both as `new Foo(...)` and
//! `Foo(...)`, emit one diagnostic per inconsistent call site.
//!
//! Classes are excluded — they are always called with `new` (the grammar
//! uses `class_declaration`, not `function_declaration`). Arrow functions
//! and `const foo = function() {}` are also excluded: arrows cannot be
//! constructed at all (the engine throws on `new`), and named function
//! expressions are rare enough that the extra noise isn't worth it.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Run once per file, at the root.
    if node.kind() != "program" {
        return;
    }

    // 1. Collect every top-level `function_declaration` name.
    let mut declared: HashMap<String, tree_sitter::Range> = HashMap::new();
    collect_function_declarations(node, source, &mut declared);
    if declared.is_empty() {
        return;
    }

    // 2. Scan every call site in the file. For each declared name, track
    //    `new`-calls and plain calls separately.
    let mut new_calls: HashMap<String, Vec<tree_sitter::Range>> = HashMap::new();
    let mut plain_calls: HashMap<String, Vec<tree_sitter::Range>> = HashMap::new();
    collect_calls(node, source, &declared, &mut new_calls, &mut plain_calls);

    // 3. For every function called in BOTH styles, emit a diagnostic on
    //    the minority set — or both sets if they are the same size — so
    //    the user sees every inconsistent call site.
    for (name, decl_range) in &declared {
        let news = new_calls.get(name);
        let plains = plain_calls.get(name);
        let (Some(news), Some(plains)) = (news, plains) else { continue };
        if news.is_empty() || plains.is_empty() {
            continue;
        }

        let decl_line = decl_range.start_point.row + 1;
        for range in news.iter().chain(plains.iter()) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: range.start_point.row + 1,
                column: range.start_point.column + 1,
                rule_id: "inconsistent-function-call".into(),
                message: format!(
                    "Function `{name}` (declared on line {decl_line}) is called both with and without `new`. Pick one style — use `new` for constructors, never for plain functions."
                ),
                severity: Severity::Error,
                span: Some((range.start_byte, range.end_byte - range.start_byte)),
            });
        }
    }
}

/// Walk the tree and record every `function_declaration` name with its
/// source range. Nested declarations count too (a function declared inside
/// another function is still a function).
fn collect_function_declarations(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    out: &mut HashMap<String, tree_sitter::Range>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "function_declaration"
            && let Some(name_node) = node.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(source)
        {
            out.entry(name.to_string()).or_insert_with(|| node.range());
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// Walk the tree and bucket every call site by name:
/// * `new Foo(...)` → `new_calls["Foo"]`
/// * `Foo(...)`     → `plain_calls["Foo"]`
///
/// Only names declared via `function_declaration` are tracked (the caller
/// passes in the set).
fn collect_calls(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    declared: &HashMap<String, tree_sitter::Range>,
    new_calls: &mut HashMap<String, Vec<tree_sitter::Range>>,
    plain_calls: &mut HashMap<String, Vec<tree_sitter::Range>>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "new_expression" => {
                if let Some(callee) = node.child_by_field_name("constructor")
                    && callee.kind() == "identifier"
                    && let Ok(name) = callee.utf8_text(source)
                    && declared.contains_key(name)
                {
                    new_calls
                        .entry(name.to_string())
                        .or_default()
                        .push(node.range());
                }
            }
            "call_expression" => {
                if let Some(callee) = node.child_by_field_name("function")
                    && callee.kind() == "identifier"
                    && let Ok(name) = callee.utf8_text(source)
                    && declared.contains_key(name)
                {
                    plain_calls
                        .entry(name.to_string())
                        .or_default()
                        .push(node.range());
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_mixed_new_and_plain_call() {
        let src = r#"
function Widget() { this.id = 1; }
const a = new Widget();
const b = Widget();
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|x| x.message.contains("Widget")));
    }

    #[test]
    fn allows_only_new_calls() {
        let src = r#"
function Widget() { this.id = 1; }
const a = new Widget();
const b = new Widget();
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_only_plain_calls() {
        let src = r#"
function helper(x) { return x + 1; }
const a = helper(1);
const b = helper(2);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_classes() {
        // Classes are always called with `new`; even if someone tries
        // `MyClass()` the grammar treats the class body separately.
        let src = r#"
class MyClass { constructor() {} }
const a = new MyClass();
const b = new MyClass();
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_arrow_functions() {
        let src = r#"
const toId = (x) => x.id;
const a = toId({ id: 1 });
const b = toId({ id: 2 });
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn handles_multiple_functions_independently() {
        let src = r#"
function Widget() { this.id = 1; }
function helper(x) { return x; }
const a = new Widget();
const b = new Widget();
const c = helper(1);
const d = helper(2);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_only_the_mixed_one() {
        let src = r#"
function Widget() { this.id = 1; }
function helper(x) { return x; }
const a = new Widget();
const b = Widget();
const c = helper(1);
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|x| x.message.contains("Widget")));
    }

    #[test]
    fn flags_three_way_imbalance() {
        // Two `new`, one plain — all three sites are inconsistent, so we
        // expect three diagnostics.
        let src = r#"
function Widget() { this.id = 1; }
const a = new Widget();
const b = new Widget();
const c = Widget();
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 3);
    }
}

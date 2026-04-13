//! no-for-loop backend — flag classic `for (let i = 0; i < arr.length; i++)`
//! patterns that can be replaced with `for (const item of arr)`.
//!
//! Detection heuristic (mirrors eslint-plugin-unicorn/no-for-loop):
//!   1. `for_statement` with `initializer` that is a `variable_declaration`
//!      declaring a single variable initialised to `0`.
//!   2. `condition` is `i < arr.length` (or `arr.length > i`).
//!   3. `increment` is `i++`, `++i`, `i += 1`, or `i = i + 1`.

use crate::diagnostic::{Diagnostic, Severity};

/// Extract text from a tree-sitter node.
fn text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

/// Check if a node is the literal `0`.
fn is_literal_zero(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.kind() == "number" && text(node, source) == "0"
}

/// Check if a node is the literal `1`.
fn is_literal_one(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.kind() == "number" && text(node, source) == "1"
}

/// Try to extract the index variable name from the `for` initialiser.
/// Expects `let i = 0` or `var i = 0`.
fn get_index_name<'a>(init: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if init.kind() != "variable_declaration" && init.kind() != "lexical_declaration" {
        return None;
    }
    // Must have exactly one declarator.
    let mut declarators = 0u32;
    let mut cursor = init.walk();
    let mut declarator = None;
    for child in init.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            declarators += 1;
            declarator = Some(child);
        }
    }
    if declarators != 1 {
        return None;
    }
    let decl = declarator?;
    let name_node = decl.child_by_field_name("name")?;
    let value_node = decl.child_by_field_name("value")?;
    if name_node.kind() != "identifier" || !is_literal_zero(value_node, source) {
        return None;
    }
    Some(text(name_node, source))
}

/// Check that the condition is `i < arr.length` (or `arr.length > i`).
fn check_condition(cond: tree_sitter::Node, source: &[u8], idx_name: &str) -> bool {
    if cond.kind() != "binary_expression" {
        return false;
    }
    let op = cond
        .child_by_field_name("operator")
        .map(|n| text(n, source))
        .unwrap_or("");
    // Also handle ternary-style: tree-sitter may represent the operator
    // as a child text.  Fallback: scan children for "<" or ">".
    let op = if op.is_empty() {
        let full = text(cond, source);
        if full.contains('<') {
            "<"
        } else if full.contains('>') {
            ">"
        } else {
            ""
        }
    } else {
        op
    };

    let left = cond.child_by_field_name("left");
    let right = cond.child_by_field_name("right");
    let (left, right) = match (left, right) {
        (Some(l), Some(r)) => (l, r),
        _ => return false,
    };

    let (lesser, greater) = match op {
        "<" => (left, right),
        ">" => (right, left),
        _ => return false,
    };

    // lesser must be the index identifier
    if text(lesser, source) != idx_name {
        return false;
    }

    // greater must be `arr.length` — a member_expression with property == "length"
    if greater.kind() != "member_expression" {
        return false;
    }
    let prop = greater.child_by_field_name("property");
    matches!(prop, Some(p) if text(p, source) == "length")
}

/// Check that the update is `i++`, `++i`, `i += 1`, or `i = i + 1`.
fn check_update(update: tree_sitter::Node, source: &[u8], idx_name: &str) -> bool {
    match update.kind() {
        "update_expression" => {
            // i++ or ++i
            let full = text(update, source).trim();
            full == format!("{idx_name}++") || full == format!("++{idx_name}")
        }
        "assignment_expression" | "augmented_assignment_expression" => {
            let left = update.child_by_field_name("left");
            let right = update.child_by_field_name("right");
            let left_text = left.map(|n| text(n, source)).unwrap_or("");
            if left_text != idx_name {
                return false;
            }
            let op_text = text(update, source);
            if op_text.contains("+=") {
                // i += 1
                return right.is_some_and(|r| is_literal_one(r, source));
            }
            if op_text.contains('=') && !op_text.contains("==") {
                // i = i + 1  or  i = 1 + i
                if let Some(r) = right
                    && r.kind() == "binary_expression" {
                        let rl = r.child_by_field_name("left");
                        let rr = r.child_by_field_name("right");
                        return match (rl, rr) {
                            (Some(a), Some(b)) => {
                                (text(a, source) == idx_name && is_literal_one(b, source))
                                    || (is_literal_one(a, source) && text(b, source) == idx_name)
                            }
                            _ => false,
                        };
                    }
            }
            false
        }
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "for_statement" {
        return;
    }

    // 1. Extract index variable name from initialiser.
    let Some(init) = node.child_by_field_name("initializer") else { return };
    let Some(idx_name) = get_index_name(init, source) else { return };

    // 2. Check the condition: `i < arr.length`.
    let Some(cond) = node.child_by_field_name("condition") else { return };
    if !check_condition(cond, source, idx_name) {
        return;
    }

    // 3. Check the increment: `i++`, `++i`, `i += 1`, `i = i + 1`.
    let Some(update) = node.child_by_field_name("increment") else { return };
    if !check_update(update, source, idx_name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-for-loop".into(),
        message: "Use a `for-of` loop instead of this `for` loop.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_classic_for_loop() {
        let d = run_on("for (let i = 0; i < arr.length; i++) { console.log(arr[i]); }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-for-loop");
    }

    #[test]
    fn flags_var_for_loop() {
        let d = run_on("for (var i = 0; i < items.length; i++) { use(items[i]); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_plus_equals_increment() {
        let d = run_on("for (let i = 0; i < arr.length; i += 1) { f(arr[i]); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_prefix_increment() {
        let d = run_on("for (let i = 0; i < arr.length; ++i) { f(arr[i]); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_reversed_condition() {
        let d = run_on("for (let i = 0; arr.length > i; i++) { f(arr[i]); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_for_of() {
        assert!(run_on("for (const item of arr) { console.log(item); }").is_empty());
    }

    #[test]
    fn allows_for_in() {
        assert!(run_on("for (const key in obj) { console.log(key); }").is_empty());
    }

    #[test]
    fn allows_non_zero_init() {
        assert!(run_on("for (let i = 1; i < arr.length; i++) { f(arr[i]); }").is_empty());
    }

    #[test]
    fn allows_decrement() {
        assert!(run_on("for (let i = 0; i < arr.length; i--) { f(arr[i]); }").is_empty());
    }

    #[test]
    fn allows_step_two() {
        assert!(run_on("for (let i = 0; i < arr.length; i += 2) { f(arr[i]); }").is_empty());
    }

    #[test]
    fn allows_non_length_condition() {
        assert!(run_on("for (let i = 0; i < 10; i++) { f(i); }").is_empty());
    }
}

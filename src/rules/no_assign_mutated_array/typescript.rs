//! no-assign-mutated-array backend — flag assignments whose RHS is a mutating
//! array method call (`sort`, `reverse`, `fill`). Mutating methods return the
//! (mutated) receiver, so capturing the return value is almost always a mistake
//! — the caller usually wants a non-mutating copy instead.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

const MUTATING_METHODS: &[&str] = &["sort", "reverse", "fill"];

/// Walk through parenthesized expressions / type assertions to reach the
/// underlying call expression node.
fn unwrap_expr<'a>(node: Node<'a>) -> Node<'a> {
    let mut cur = node;
    loop {
        match cur.kind() {
            "parenthesized_expression" | "as_expression" | "satisfies_expression"
            | "type_assertion" | "non_null_expression" => {
                let Some(inner) = cur.named_child(0) else { return cur };
                cur = inner;
            }
            _ => return cur,
        }
    }
}

fn mutating_method_name<'a>(call: Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if call.kind() != "call_expression" {
        return None;
    }
    let callee = call.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let prop = callee.child_by_field_name("property")?;
    let name = prop.utf8_text(source).ok()?;
    if !MUTATING_METHODS.contains(&name) {
        return None;
    }

    // Allow when the receiver is a freshly-created array — the mutation is
    // confined to a temporary, so assigning the result is safe. Covers:
    //   [...arr].sort()
    //   Array.from(arr).sort()
    //   arr.slice().sort()
    //   arr.filter(...).sort()
    //   arr.map(...).sort()
    //   arr.concat(...).sort()
    let object = callee.child_by_field_name("object")?;
    let recv = unwrap_expr(object);
    if is_fresh_array(recv, source) {
        return None;
    }

    Some(name)
}

fn is_fresh_array(node: Node<'_>, source: &[u8]) -> bool {
    match node.kind() {
        "array" => true,
        "call_expression" => {
            let Some(fun) = node.child_by_field_name("function") else { return false };
            match fun.kind() {
                "member_expression" => {
                    let Some(prop) = fun.child_by_field_name("property") else { return false };
                    matches!(
                        prop.utf8_text(source).unwrap_or(""),
                        "slice" | "filter" | "map" | "concat" | "flat" | "flatMap"
                            | "toSorted" | "toReversed" | "toSpliced" | "with"
                    )
                }
                _ => false,
            }
        }
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let rhs = match node.kind() {
        "variable_declarator" => node.child_by_field_name("value"),
        "assignment_expression" => node.child_by_field_name("right"),
        _ => return,
    };
    let Some(rhs) = rhs else { return };

    let call = unwrap_expr(rhs);
    let Some(method) = mutating_method_name(call, source) else { return };

    let pos = call.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-assign-mutated-array".into(),
        message: format!(
            "Assigning result of `.{method}()` — mutating method returns the same array. \
             Use `toSorted()`, `toReversed()`, or spread before mutating: `[...arr].{method}(...)`."
        ),
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
    fn flags_const_sort() {
        assert_eq!(run_on("const x = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_const_reverse() {
        assert_eq!(run_on("const x = arr.reverse();").len(), 1);
    }

    #[test]
    fn flags_const_fill() {
        assert_eq!(run_on("const x = arr.fill(0);").len(), 1);
    }

    #[test]
    fn flags_let_sort_with_comparator() {
        assert_eq!(run_on("let x = items.sort((a, b) => a - b);").len(), 1);
    }

    #[test]
    fn flags_reassignment() {
        assert_eq!(run_on("x = arr.reverse();").len(), 1);
    }

    #[test]
    fn allows_to_sorted() {
        assert!(run_on("const x = arr.toSorted();").is_empty());
    }

    #[test]
    fn allows_to_reversed() {
        assert!(run_on("const x = arr.toReversed();").is_empty());
    }

    #[test]
    fn allows_inline_sort_without_assignment() {
        assert!(run_on("arr.sort();").is_empty());
    }

    #[test]
    fn allows_spread_then_sort() {
        assert!(run_on("const x = [...arr].sort();").is_empty());
    }
}

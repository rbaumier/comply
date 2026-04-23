//! no-mutating-methods backend — flag calls to array mutating methods.
//!
//! We match any `x.method(...)` call where `method` is a known
//! mutating array method. This is a name-based heuristic — we cannot
//! resolve the receiver's type — but these names are overwhelmingly
//! used on arrays, and each has an explicit non-mutating alternative.

use crate::diagnostic::{Diagnostic, Severity};

const MUTATING: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Ok(name) = prop.utf8_text(source) else { return };
    if !MUTATING.contains(&name) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-mutating-methods",
        format!(
            "`.{name}()` mutates the array in place — use a non-mutating alternative (spread, `slice`, `toSorted`, `toReversed`, `toSpliced`, `filter`, `map`, `concat`)."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_push() {
        assert_eq!(run_on("arr.push(1);").len(), 1);
    }

    #[test]
    fn flags_sort() {
        assert_eq!(run_on("arr.sort();").len(), 1);
    }

    #[test]
    fn flags_splice() {
        assert_eq!(run_on("arr.splice(0, 1);").len(), 1);
    }

    #[test]
    fn flags_reverse() {
        assert_eq!(run_on("arr.reverse();").len(), 1);
    }

    #[test]
    fn allows_non_mutating_alternatives() {
        assert!(run_on("const next = [...arr, 1];").is_empty());
        assert!(run_on("arr.toSorted();").is_empty());
        assert!(run_on("arr.toReversed();").is_empty());
        assert!(run_on("arr.slice(0, 1);").is_empty());
        assert!(run_on("arr.map(x => x + 1);").is_empty());
    }

    #[test]
    fn ignores_plain_function_call() {
        assert!(run_on("push(arr, 1);").is_empty());
    }
}

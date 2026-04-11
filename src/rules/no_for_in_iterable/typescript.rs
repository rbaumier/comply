//! no-for-in-iterable backend — flag `for...in` on arrays/iterables.

use crate::diagnostic::{Diagnostic, Severity};

/// Heuristic names that suggest an array/iterable.
const ITERABLE_HINTS: &[&str] = &[
    "arr", "list", "items", "elements", "array",
    "values", "entries", "results", "rows", "records",
];

/// Heuristic: the right-hand side looks like an array/iterable.
fn looks_like_iterable(rhs: &str) -> bool {
    if rhs.starts_with('[') {
        return true;
    }
    let rhs_lower = rhs.to_ascii_lowercase();
    ITERABLE_HINTS.iter().any(|hint| rhs_lower.contains(hint))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "for_in_statement" {
        return;
    }
    // tree-sitter parses both `for..in` and `for..of` as `for_in_statement`.
    // Distinguish by looking for the `of` keyword child.
    let mut cursor = node.walk();
    let is_for_of = node.children(&mut cursor).any(|c| c.kind() == "of");
    if is_for_of {
        return;
    }
    let Some(right) = node.child_by_field_name("right") else { return };
    let Ok(rhs_text) = right.utf8_text(source) else { return };
    if !looks_like_iterable(rhs_text) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-for-in-iterable".into(),
        message: "`for...in` on an array/iterable — use `for...of` instead.".into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_for_in_with_array_name() {
        assert_eq!(run_on("for (const x in myArray) {}").len(), 1);
    }

    #[test]
    fn flags_for_in_with_list_name() {
        assert_eq!(run_on("for (let key in itemsList) {}").len(), 1);
    }

    #[test]
    fn allows_for_in_with_object() {
        assert!(run_on("for (const key in obj) {}").is_empty());
    }

    #[test]
    fn allows_for_of() {
        assert!(run_on("for (const x of myArray) {}").is_empty());
    }
}

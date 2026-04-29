//! no-array-sort-mutation backend — flag `.sort()` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = [".sort"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if method != "sort" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-array-sort-mutation".into(),
        message: "Use `.toSorted()` instead of `.sort()` — `sort()` mutates the array in place.".into(),
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
    fn flags_sort_without_comparator() {
        assert_eq!(run_on("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_with_comparator() {
        assert_eq!(run_on("arr.sort((a, b) => a - b);").len(), 1);
    }

    #[test]
    fn allows_to_sorted() {
        assert!(run_on("const sorted = arr.toSorted();").is_empty());
    }

    #[test]
    fn allows_to_sorted_with_comparator() {
        assert!(run_on("const sorted = arr.toSorted((a, b) => a - b);").is_empty());
    }

    #[test]
    fn flags_chained_sort() {
        assert_eq!(run_on("const sorted = items.filter(x => x).sort();").len(), 1);
    }
}

//! tanstack-query-array-key backend — flag `queryKey: 'some-string'`.
//!
//! Why: TanStack Query v5 requires query keys to be arrays. Strings
//! silently work in some versions and break in others. An array key is
//! also required for hierarchical invalidation: `['todos', id]` lets
//! `invalidateQueries({ queryKey: ['todos'] })` match everything.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { prefilter = ["queryKey", "mutationKey"] => |node, source, ctx, diagnostics|
    let Some((key, _)) = crate::rules::object_literal::object_pair(node, source) else {
        return;
    };
    if key != "queryKey" && key != "mutationKey" {
        return;
    }
    let Some(value_node) = node.child_by_field_name("value") else {
        return;
    };
    if !matches!(value_node.kind(), "string" | "template_string") {
        return;
    }
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "tanstack-query-array-key".into(),
        message: format!(
            "`{key}` must be an array. Wrap the string in brackets: `['todos']` \
             instead of `'todos'`. Array keys enable hierarchical invalidation."
        ),
        severity: Severity::Error,
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
    fn flags_string_query_key() {
        assert_eq!(
            run_on("useQuery({ queryKey: 'todos', queryFn: f });").len(),
            1
        );
    }

    #[test]
    fn allows_array_query_key() {
        assert!(run_on("useQuery({ queryKey: ['todos'], queryFn: f });").is_empty());
    }
}

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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

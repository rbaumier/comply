//! no-sort-without-comparator backend — `.sort()` called with no comparator.
//!
//! Walks `call_expression` nodes whose function is `<expr>.sort` and whose
//! arguments list is empty. A bare `.sort()` sorts lexicographically, which
//! silently breaks numeric arrays.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["sort"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "sort" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 0 {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-sort-without-comparator",
        "`.sort()` without comparator sorts lexicographically — pass an explicit compare function.".into(),
        Severity::Error,
    ));
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
    fn flags_empty_sort() {
        assert_eq!(run_on("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_with_whitespace() {
        assert_eq!(run_on("const sorted = arr.sort(  );").len(), 1);
    }

    #[test]
    fn allows_sort_with_comparator() {
        assert!(run_on("const sorted = arr.sort((a, b) => a - b);").is_empty());
    }
}

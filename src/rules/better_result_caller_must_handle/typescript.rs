use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["expression_statement"] prefilter = ["better-result"] => |node, source, ctx, diagnostics|
    if !super::imports_better_result(ctx.source) {
        return;
    }
    let mut cursor = node.walk();
    let Some(inner) = node.children(&mut cursor).find(|c| c.kind() == "call_expression") else {
        return;
    };
    let Some(callee) = inner.child_by_field_name("function") else { return; };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !super::returns_result(callee_text) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Returned Result from `{callee_text}(...)` is ignored — assign, match, map, unwrap, or yield* it."),
        Severity::Warning,
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
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }
    #[test]
    fn flags_ignored_result_call() {
        let src = "import { Result } from 'better-result';\nfindUserResult(id);";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_assigned_result() {
        let src = "import { Result } from 'better-result';\nconst r = findUserResult(id);";
        assert!(run(src).is_empty());
    }
    #[test]
    fn flags_try_prefixed_call() {
        let src = "import { Result } from 'better-result';\ntryFetchUser(id);";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn flags_attempt_prefixed_call() {
        let src = "import { Result } from 'better-result';\nattemptParse(input);";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn flags_safe_prefixed_call() {
        let src = "import { Result } from 'better-result';\nsafeDivide(a, b);";
        assert_eq!(run(src).len(), 1);
    }
    /// Documents the heuristic limitation: a function returning `Result<T, E>`
    /// whose name doesn't follow the `Result` / `try*` / `attempt*` / `safe*`
    /// convention is *not* flagged. Detecting it would require type info.
    #[test]
    fn limitation_does_not_flag_arbitrary_returning_call() {
        let src = "import { Result } from 'better-result';\nfindUser(id);";
        assert!(run(src).is_empty());
    }
}

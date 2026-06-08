//! require-array-join-separator AST backend.
//!
//! Flags `.join()` calls with no separator argument.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // callee must be `*.join`
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "join" {
        return;
    }

    // arguments: must have zero arguments
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 0 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "require-array-join-separator".into(),
        message: "Missing the separator argument in `.join()` \u{2014} use `.join(',')` explicitly.".into(),
        severity: Severity::Warning,
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
    fn flags_empty_join() {
        assert_eq!(run_on("const s = arr.join();").len(), 1);
    }

    #[test]
    fn flags_chained_join() {
        assert_eq!(run_on("foo.map(x => x.id).join()").len(), 1);
    }

    #[test]
    fn allows_join_with_separator() {
        assert!(run_on("const s = arr.join(',');").is_empty());
    }

    #[test]
    fn allows_join_with_variable() {
        assert!(run_on("const s = arr.join(sep);").is_empty());
    }
}

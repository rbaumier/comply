//! no-try-statements backend — flag `try` blocks.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["try_statement"] prefilter = ["try"] => |node, source, ctx, diagnostics|
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-try-statements".into(),
        message: "`try` block \u{2014} prefer Result types or explicit error handling.".into(),
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

    #[test]
    fn flags_try_block() {
        let d = crate::rules::test_helpers::run_rule(&Check, "try { foo(); } catch (e) {}", "t.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-try-statements");
    }

    #[test]
    fn flags_try_finally() {
        let d = crate::rules::test_helpers::run_rule(&Check, "try { foo(); } finally {}", "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_normal_code() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const retry = 3;", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_function_call() {
        let d = crate::rules::test_helpers::run_rule(&Check, "doSomething();", "t.ts");
        assert!(d.is_empty());
    }
}

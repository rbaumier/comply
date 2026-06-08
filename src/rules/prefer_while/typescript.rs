//! prefer-while backend — flag `for(;;)` / `for(;cond;)` without init/update.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["for_statement"] => |node, source, ctx, diagnostics|
    // A for_statement has fields: initializer, condition, increment, body.
    // tree-sitter always provides `initializer` (as empty_statement when omitted).
    // `increment` is None when omitted.
    let has_init = node.child_by_field_name("initializer")
        .is_some_and(|n| n.kind() != "empty_statement");
    let has_increment = node.child_by_field_name("increment").is_some();

    if has_init || has_increment {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-while".into(),
        message: "Use `while` instead of `for` without init/update.".into(),
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
    fn flags_for_infinite() {
        assert_eq!(run_on("for (;;) {}").len(), 1);
    }

    #[test]
    fn flags_for_condition_only() {
        assert_eq!(run_on("for (;x < 10;) {}").len(), 1);
    }

    #[test]
    fn allows_standard_for_loop() {
        assert!(run_on("for (let i = 0; i < 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_while_true() {
        assert!(run_on("while (true) {}").is_empty());
    }
}

//! no-constructor-side-effects — flag `new X()` used as a statement
//! (not assigned, returned, thrown, etc.).
//!
//! Matches `expression_statement` nodes whose expression is a
//! `new_expression`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["expression_statement"] => |node, source, ctx, diagnostics|
    let Some(expr) = node.named_child(0) else { return };

    if expr.kind() != "new_expression" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-constructor-side-effects".into(),
        message: "`new X()` without assignment — constructors should not be called for side effects.".into(),
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
    fn flags_standalone_new() {
        assert_eq!(run_on("new MyService();").len(), 1);
    }

    #[test]
    fn flags_standalone_new_indented() {
        assert_eq!(run_on("  new MyService();").len(), 1);
    }

    #[test]
    fn allows_assigned_new() {
        assert!(run_on("const svc = new MyService();").is_empty());
    }

    #[test]
    fn allows_returned_new() {
        assert!(run_on("function f() { return new MyService(); }").is_empty());
    }

    #[test]
    fn allows_thrown_new() {
        assert!(run_on("throw new Error('fail');").is_empty());
    }
}

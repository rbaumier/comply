//! no-nested-incdec backend — flag `++`/`--` used inside expressions
//! rather than as standalone statements.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["update_expression"] => |node, _source, ctx, diagnostics|
    let Some(parent) = node.parent() else {
        return;
    };
    // Standalone: update_expression is the direct child of expression_statement
    if parent.kind() == "expression_statement" {
        return;
    }
    // For-loop update clause: update_expression in for_statement's increment field
    if parent.kind() == "for_statement" {
        return;
    }
    // Also allow inside sequence_expression that is itself in a for_statement increment
    if parent.kind() == "sequence_expression"
        && let Some(grandparent) = parent.parent()
        && grandparent.kind() == "for_statement"
    {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-nested-incdec".into(),
        message: "`++`/`--` inside an expression — separate into its own statement for clarity.".into(),
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
    fn flags_incdec_in_array_index() {
        assert_eq!(run_on("arr[i++] = x;").len(), 1);
    }

    #[test]
    fn flags_incdec_in_function_call() {
        assert_eq!(run_on("f(x++);").len(), 1);
    }

    #[test]
    fn allows_standalone_postfix() {
        assert!(run_on("i++;").is_empty());
    }

    #[test]
    fn allows_standalone_prefix() {
        assert!(run_on("++i;").is_empty());
    }

    #[test]
    fn allows_for_loop_update() {
        assert!(run_on("for (let i = 0; i < n; i++) {}").is_empty());
    }
}

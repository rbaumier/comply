use crate::diagnostic::{Diagnostic, Severity};

const LOOP_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
];

crate::ast_check! { on ["function_declaration", "function_expression", "arrow_function"] => |node, source, ctx, diagnostics|
    // Check for function/arrow inside loop body
    // Walk up to see if we're inside a loop
    let mut current = node.parent();
    while let Some(parent) = current {
        if LOOP_KINDS.contains(&parent.kind()) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "function-inside-loop".into(),
                message: "Function declared inside loop — creates new function object each iteration.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }
        // Stop at function boundaries — nested functions are OK
        if parent.kind() == "function_declaration"
            || parent.kind() == "function_expression"
            || parent.kind() == "arrow_function"
            || parent.kind() == "method_definition" {
            return;
        }
        current = parent.parent();
    }
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
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    #[test]
    fn flags_function_in_for() {
        assert_eq!(
            run("for (let i = 0; i < 10; i++) { function foo() {} }").len(),
            1
        );
    }

    #[test]
    fn flags_arrow_in_for() {
        assert_eq!(
            run("for (let i = 0; i < 10; i++) { const fn = () => i; }").len(),
            1
        );
    }

    #[test]
    fn flags_function_in_while() {
        assert_eq!(run("while (true) { const fn = function() {}; }").len(), 1);
    }

    #[test]
    fn allows_function_outside_loop() {
        assert!(run("function foo() {} for (let i = 0; i < 10; i++) { foo(); }").is_empty());
    }

    #[test]
    fn allows_method_reference() {
        assert!(run("for (const item of items) { item.map(process); }").is_empty());
    }
}

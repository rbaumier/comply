//! ts-no-loop-func backend — flag function declarations/expressions and
//! arrow functions that appear inside loop bodies.
//!
//! Detection: walk function nodes and check if any ancestor is a loop body.

use crate::diagnostic::{Diagnostic, Severity};

const LOOP_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
];

fn is_inside_loop(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        let kind = ancestor.kind();
        // Stop at function boundaries — nested functions don't count.
        if kind == "function_declaration"
            || kind == "function_expression"
            || kind == "arrow_function"
            || kind == "method_definition"
        {
            return false;
        }
        if LOOP_KINDS.contains(&kind) {
            return true;
        }
        current = ancestor.parent();
    }
    false
}

crate::ast_check! { |node, _source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "function_declaration"
        && kind != "function_expression"
        && kind != "arrow_function"
    {
        return;
    }

    if !is_inside_loop(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-loop-func".into(),
        message: "Function declared inside a loop — closures may \
                  capture the loop variable by reference. Move it outside."
            .into(),
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
    fn flags_function_in_for_loop() {
        let diags = run_on("for (var i = 0; i < 10; i++) { function foo() { return i; } }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_arrow_in_while_loop() {
        let diags = run_on("while (true) { const fn = () => 1; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_function_outside_loop() {
        assert!(run_on("function foo() { return 1; }").is_empty());
    }
}

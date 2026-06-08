//! no-function-declaration-in-block backend — flag function declarations
//! inside control-flow blocks (if, for, while, switch).

use crate::diagnostic::{Diagnostic, Severity};

const CONTROL_FLOW_KINDS: &[&str] = &[
    "if_statement",
    "else_clause",
    "for_statement",
    "for_in_statement",
    "for_of_statement",
    "while_statement",
    "do_statement",
    "switch_case",
    "switch_default",
];

/// Walk up from a node checking if any ancestor is a control-flow block.
fn is_inside_control_flow(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if CONTROL_FLOW_KINDS.contains(&parent.kind()) {
            return true;
        }
        // Stop at program/module root
        if parent.kind() == "program" {
            return false;
        }
        current = parent.parent();
    }
    false
}

crate::ast_check! { on ["function_declaration"] => |node, _source, ctx, diagnostics|
    if !is_inside_control_flow(node) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-function-declaration-in-block".into(),
        message: "Function declaration inside a control-flow block — move it to the top level or use a function expression.".into(),
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
    fn flags_function_in_if_block() {
        let src = "if (true) {\n  function foo() {}\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_function_in_for_block() {
        let src = "for (let i = 0; i < 10; i++) {\n  function bar() { return i; }\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_top_level_function() {
        let src = "function baz() {\n  return 1;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_in_block() {
        let src = "if (true) {\n  const fn = () => {};\n}";
        assert!(run_on(src).is_empty());
    }
}

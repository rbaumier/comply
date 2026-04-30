//! no-nested-ternary backend for TypeScript / JavaScript / TSX.
//!
//! Walks the AST for `ternary_expression` nodes whose parent is also a
//! `ternary_expression` — exactly the "nested" shape that's hard to read
//! and easy to misparse visually.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["ternary_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let parent_is_ternary = node
            .parent()
            .is_some_and(|p| p.kind() == "ternary_expression");
        if !parent_is_ternary {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-nested-ternary".into(),
            message: "Nested ternary — extract to if/else or a named variable for each branch."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_nested_ternary() {
        let diags = run_on("const x = a ? b ? 1 : 2 : 3;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_single_ternary() {
        assert!(run_on("const x = a ? 1 : 2;").is_empty());
    }

    #[test]
    fn flags_deeply_nested_ternaries() {
        assert_eq!(run_on("const x = a ? b ? c ? 1 : 2 : 3 : 4;").len(), 2);
    }
}

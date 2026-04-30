//! react-no-and-conditional-jsx backend — flag `{expr && <Jsx />}` inside JSX.
//!
//! Why: `&&` short-circuits on any falsy value, including `0` and `""`.
//! `{items.length && <List />}` renders `0` to the DOM when the list is
//! empty — not what you wanted. Use a ternary: `{items.length > 0 ? <List /> : null}`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["binary_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        // Must be directly inside a jsx_expression.
        let Some(parent) = node.parent() else {
            return;
        };
        if parent.kind() != "jsx_expression" {
            return;
        }
        let Some(operator) = node.child_by_field_name("operator") else {
            return;
        };
        let Ok(op_text) = operator.utf8_text(source_bytes) else {
            return;
        };
        if op_text != "&&" {
            return;
        }
        // Right side must be JSX (that's the rendering pattern).
        let Some(right) = node.child_by_field_name("right") else {
            return;
        };
        if !matches!(right.kind(), "jsx_element" | "jsx_self_closing_element") {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-no-and-conditional-jsx".into(),
            message: "`{expr && <X />}` renders `0` or `''` when expr \
                      is falsy-but-not-false. Use a ternary: \
                      `{expr ? <X /> : null}`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_and_conditional_jsx() {
        assert_eq!(
            run_on("const x = <div>{isAdmin && <Panel />}</div>;").len(),
            1
        );
    }

    #[test]
    fn allows_ternary() {
        assert!(run_on("const x = <div>{isAdmin ? <Panel /> : null}</div>;").is_empty());
    }

    #[test]
    fn does_not_flag_non_jsx_right_operand() {
        // `a && b` with both sides being values is fine outside JSX context.
        assert!(run_on("const x = <div>{a && b}</div>;").is_empty());
    }
}

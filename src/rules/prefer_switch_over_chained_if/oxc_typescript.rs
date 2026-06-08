//! prefer-switch-over-chained-if OXC backend — flag 4+ if/else-if chains.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IfStatement(if_stmt) = node.kind() else {
            return;
        };

        // Only count chain roots — skip if this if-statement is an else branch.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::IfStatement(_)) {
            return;
        }

        let min_arms = ctx
            .config
            .threshold("prefer-switch-over-chained-if", "min_arms", ctx.lang);

        let arms = count_chained_arms(if_stmt);
        if arms < min_arms {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, if_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{arms}-branch if/else-if chain — convert to a \
                 `switch` statement. Switch makes the discriminant \
                 obvious and the TypeScript compiler can warn on \
                 missing cases for union-typed values."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn count_chained_arms(stmt: &oxc_ast::ast::IfStatement) -> usize {
    let mut arms = 1;
    let mut current = stmt;
    loop {
        match &current.alternate {
            Some(Statement::IfStatement(next)) => {
                arms += 1;
                current = next;
            }
            _ => break,
        }
    }
    arms
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_four_arm_chain() {
        let source = "
function f(k: string) {
    if (k === 'a') return 1;
    else if (k === 'b') return 2;
    else if (k === 'c') return 3;
    else if (k === 'd') return 4;
}
";
        assert_eq!(run_on(source).len(), 1);
    }


    #[test]
    fn allows_three_arm_chain() {
        let source = "
function f(k: string) {
    if (k === 'a') return 1;
    else if (k === 'b') return 2;
    else if (k === 'c') return 3;
}
";
        assert!(run_on(source).is_empty());
    }


    #[test]
    fn allows_single_if() {
        assert!(run_on("function f() { if (x) return 1; }").is_empty());
    }


    #[test]
    fn does_not_double_count_nested_chain() {
        // The inner `else if` shouldn't be counted as its own chain root.
        let source = "
function f(k: string) {
    if (k === 'a') return 1;
    else if (k === 'b') return 2;
    else if (k === 'c') return 3;
    else if (k === 'd') return 4;
    else if (k === 'e') return 5;
}
";
        assert_eq!(run_on(source).len(), 1);
    }
}

//! no-nested-incdec OXC backend — flag `++`/`--` used inside expressions
//! rather than as standalone statements.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UpdateExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UpdateExpression(update) = node.kind() else {
            return;
        };

        let nodes = semantic.nodes();
        let parent_id = nodes.parent_id(node.id());
        if parent_id == node.id() {
            return;
        }
        let parent_kind = nodes.kind(parent_id);

        // Standalone: update_expression is the direct child of expression_statement
        if matches!(parent_kind, AstKind::ExpressionStatement(_)) {
            return;
        }
        // For-loop update clause
        if matches!(parent_kind, AstKind::ForStatement(_)) {
            return;
        }
        // Also allow inside sequence_expression that is itself in a for_statement increment
        if matches!(parent_kind, AstKind::SequenceExpression(_)) {
            let grandparent_id = nodes.parent_id(parent_id);
            if grandparent_id != parent_id
                && matches!(nodes.kind(grandparent_id), AstKind::ForStatement(_))
            {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, update.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`++`/`--` inside an expression — separate into its own statement for clarity."
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
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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

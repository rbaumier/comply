use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SpreadElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SpreadElement(spread) = node.kind() else {
            return;
        };

        let parent = semantic.nodes().parent_node(node.id());

        let is_useless = match parent.kind() {
            // `{...{a:1}}` — object spread of an object literal inside object
            AstKind::ObjectExpression(_) => {
                matches!(spread.argument, Expression::ObjectExpression(_))
            }
            // `[...[1,2]]` — array spread of an array literal inside array
            AstKind::ArrayExpression(_) => {
                matches!(spread.argument, Expression::ArrayExpression(_))
            }
            // `fn(...[1,2])` — array spread of an array literal inside arguments
            AstKind::CallExpression(_) | AstKind::NewExpression(_) => {
                matches!(spread.argument, Expression::ArrayExpression(_))
            }
            _ => false,
        };

        if !is_useless {
            return;
        }

        let label = if matches!(spread.argument, Expression::ArrayExpression(_)) {
            "array"
        } else {
            "object"
        };
        let container = match parent.kind() {
            AstKind::ObjectExpression(_) => "object literal",
            AstKind::ArrayExpression(_) => "array literal",
            AstKind::CallExpression(_) | AstKind::NewExpression(_) => "arguments",
            _ => "expression",
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, spread.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Spreading an {label} literal in {container} is unnecessary."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

//! prefer-regexp-test OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Check if the parent node represents a boolean context.
fn is_boolean_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    match parent.kind() {
        AstKind::IfStatement(_) | AstKind::WhileStatement(_) | AstKind::DoWhileStatement(_) => {
            true
        }
        AstKind::UnaryExpression(unary) => {
            // `!str.match(...)` or `!!str.match(...)`
            matches!(unary.operator, oxc_ast::ast::UnaryOperator::LogicalNot)
        }
        AstKind::LogicalExpression(_) => true,
        AstKind::ParenthesizedExpression(_) => {
            // Recurse up: `if ((str.match(...)))`
            is_boolean_context(parent, semantic)
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".match"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `.match`
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "match" {
            return;
        }

        // First argument must be a regex literal
        let has_regex_arg = call.arguments.first().is_some_and(|arg| {
            matches!(arg, oxc_ast::ast::Argument::RegExpLiteral(_))
        });
        if !has_regex_arg {
            return;
        }

        // Only flag if in a boolean context
        if !is_boolean_context(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `RegExp#test()` over `String#match()` in boolean contexts.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

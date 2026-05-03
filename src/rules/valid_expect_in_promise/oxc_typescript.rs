use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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

        // Callee must be `.then(...)` or `.catch(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop = member.property.name.as_str();
        if prop != "then" && prop != "catch" {
            return;
        }

        // Check if any argument callback contains `expect(...)`.
        if !args_contain_expect(&call.arguments, ctx.source) {
            return;
        }

        // Check if this call is returned or awaited.
        if is_returned_or_awaited(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Promise `.{prop}()` with `expect()` inside must be returned or awaited."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True if any descendant of the arguments contains a call to `expect(...)`.
fn args_contain_expect(args: &[Argument], source: &str) -> bool {
    for arg in args {
        use oxc_span::GetSpan;
        let span = arg.span();
        let text = &source[span.start as usize..span.end as usize];
        if text.contains("expect(") {
            return true;
        }
    }
    false
}

/// True if the call node is returned, awaited, or is an arrow function's expression body.
fn is_returned_or_awaited(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();

    loop {
        let parent = nodes.parent_node(current_id);
        if parent.id() == current_id {
            return false;
        }
        match parent.kind() {
            AstKind::ReturnStatement(_) | AstKind::AwaitExpression(_) => return true,
            AstKind::ArrowFunctionExpression(arrow) => {
                // Only counts if this is the expression body (not inside a block body).
                return arrow.expression;
            }
            // Transparent wrappers — keep climbing.
            AstKind::ParenthesizedExpression(_)
            | AstKind::TSNonNullExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSTypeAssertion(_) => {
                current_id = parent.id();
            }
            _ => return false,
        }
    }
}

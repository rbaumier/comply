//! try-catch-json-parse oxc backend — flag `JSON.parse(...)` not wrapped
//! in a `try` statement.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Walk up semantic parents to check if node is inside a try body.
/// Stop at function boundaries (outer try can't catch inner function throws
/// unless the function is awaited/called within the try).
fn is_inside_try_body<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut cur = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(cur);
        if parent_id == cur {
            break;
        }
        let parent = semantic.nodes().get_node(parent_id);
        match parent.kind() {
            AstKind::TryStatement(try_stmt) => {
                // Check if our node is inside the try block (not the catch/finally).
                let block_span = try_stmt.block.span;
                let node_start = node.kind().span().start;
                let node_end = node.kind().span().end;
                if node_start >= block_span.start && node_end <= block_span.end {
                    return true;
                }
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return false;
            }
            _ => {}
        }
        cur = parent_id;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["JSON.parse"])
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
        // Callee must be `JSON.parse`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "JSON" || member.property.name.as_str() != "parse" {
            return;
        }

        if is_inside_try_body(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`JSON.parse` can throw on invalid input — wrap it in \
                      try/catch or use a safe parser (Zod, schema validator)."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

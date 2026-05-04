//! prisma-prefer-create-many-for-bulk oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_prisma_file(source: &str) -> bool {
    source.contains("@prisma/client")
        || source.contains("PrismaClient")
        || source.contains("prisma.")
}

/// Walk ancestors to check if this node is inside a loop construct.
fn enclosing_is_loop<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = &call.callee {
                    let prop = member.property.name.as_str();
                    if matches!(prop, "forEach" | "map") {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    let _ = source;
    false
}

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
        if !is_prisma_file(ctx.source) {
            return;
        }
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "create" {
            return;
        }
        if !enclosing_is_loop(node, semantic, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`create()` called inside a loop — use `createMany({ data: [...] })` for one round-trip.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

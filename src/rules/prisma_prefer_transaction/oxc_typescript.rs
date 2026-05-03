//! prisma-prefer-transaction oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const WRITE_METHODS: &[&str] = &[
    "create", "createMany", "update", "updateMany", "delete", "deleteMany", "upsert",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@prisma/client", "PrismaClient", "prisma."])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let span = match node.kind() {
            AstKind::Function(f) => f.span,
            AstKind::ArrowFunctionExpression(a) => a.span,
            _ => return,
        };

        // Check if the function body text contains $transaction — if so, skip.
        let body_text = &ctx.source[span.start as usize..span.end as usize];
        if body_text.contains("$transaction") {
            return;
        }

        // Count Prisma write calls among descendants of this node.
        let mut writes = 0usize;
        for descendant in semantic.nodes().descendants(node.id()) {
            let AstKind::CallExpression(call) = descendant.kind() else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let method = member.property.name.as_str();
            if WRITE_METHODS.contains(&method) {
                writes += 1;
            }
        }

        if writes < 2 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{writes} Prisma write calls in this function — wrap them in `prisma.$transaction([...])` for atomicity."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

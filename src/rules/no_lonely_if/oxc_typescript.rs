//! no-lonely-if oxc backend — flag `else { if (x) { } }` that should be
//! `else if (x) { }`.

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
        let AstKind::IfStatement(stmt) = node.kind() else { return };

        // Check: is this if_statement the sole child of a block
        // that is the alternate of a parent if_statement?
        //
        // In oxc, `else { if (b) {} }` is parsed as:
        //   IfStatement (outer)
        //     alternate: BlockStatement
        //       body: [IfStatement (inner)]
        //
        // While `else if (b) {}` is parsed as:
        //   IfStatement (outer)
        //     alternate: IfStatement (inner) — no block wrapper
        //
        // So we look for: parent is BlockStatement with exactly 1 child,
        // and grandparent is IfStatement where this block is the alternate.

        let parent = semantic.nodes().parent_node(node.id());
        let AstKind::BlockStatement(block) = parent.kind() else { return };

        // The block must contain exactly one statement (this if).
        if block.body.len() != 1 {
            return;
        }

        // The block's parent must be an IfStatement.
        let grandparent = semantic.nodes().parent_node(parent.id());
        let AstKind::IfStatement(outer_if) = grandparent.kind() else { return };

        // The block must be the alternate of the outer if, not the consequent.
        // We check that the outer if has an alternate and its span matches
        // our parent block's span.
        let Some(Statement::BlockStatement(alt_block)) = &outer_if.alternate else { return };
        if alt_block.span != block.span {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected `if` as the only statement in an `else` block \
                      — use `else if` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

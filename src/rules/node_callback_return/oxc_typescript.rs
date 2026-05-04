//! node-callback-return OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const CALLBACKS: &[&str] = &["callback", "cb", "next"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["callback", "cb", "next"])
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

        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if !CALLBACKS.contains(&callee.name.as_str()) {
            return;
        }

        // Walk up to find the parent statement context.
        let parent = semantic.nodes().parent_node(node.id());
        match parent.kind() {
            // `return cb(err);`
            AstKind::ReturnStatement(_) => return,
            // Arrow body: `(err) => cb(err)`
            AstKind::ArrowFunctionExpression(_) => return,
            AstKind::ExpressionStatement(expr_stmt) => {
                let grandparent = semantic.nodes().parent_node(parent.id());
                if let AstKind::FunctionBody(block) = grandparent.kind() {
                    let stmts = &block.statements;
                    // Find our position in the block.
                    let our_span = expr_stmt.span;
                    let mut found_idx = None;
                    for (i, s) in stmts.iter().enumerate() {
                        if s.span() == our_span {
                            found_idx = Some(i);
                            break;
                        }
                    }
                    if let Some(idx) = found_idx {
                        // Check if the next statement is a return or throw.
                        if let Some(next) = stmts.get(idx + 1) {
                            if matches!(
                                next,
                                Statement::ReturnStatement(_) | Statement::ThrowStatement(_)
                            ) {
                                return;
                            }
                        }

                        // If this is the last statement in a function body, it's fine.
                        if idx == stmts.len() - 1 {
                            let great_grandparent =
                                semantic.nodes().parent_node(grandparent.id());
                            if matches!(
                                great_grandparent.kind(),
                                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
                            ) {
                                return;
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Expected `return` with your callback function.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

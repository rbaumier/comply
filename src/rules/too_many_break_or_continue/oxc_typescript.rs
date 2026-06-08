//! too-many-break-or-continue oxc backend — flag loops with 2+ break/continue.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

/// Count `break` and `continue` statements that belong directly to this loop
/// (not nested inside inner loops).
fn count_break_continue(stmts: &[Statement]) -> usize {
    let mut count = 0;
    for stmt in stmts {
        match stmt {
            Statement::BreakStatement(_) | Statement::ContinueStatement(_) => {
                count += 1;
            }
            // Don't recurse into nested loops.
            Statement::ForStatement(_)
            | Statement::ForInStatement(_)
            | Statement::ForOfStatement(_)
            | Statement::WhileStatement(_)
            | Statement::DoWhileStatement(_) => {}
            Statement::BlockStatement(block) => {
                count += count_break_continue(&block.body);
            }
            Statement::IfStatement(if_stmt) => {
                count += count_break_continue_stmt(&if_stmt.consequent);
                if let Some(alt) = &if_stmt.alternate {
                    count += count_break_continue_stmt(alt);
                }
            }
            Statement::LabeledStatement(l) => {
                count += count_break_continue_stmt(&l.body);
            }
            Statement::TryStatement(t) => {
                count += count_break_continue(&t.block.body);
                if let Some(h) = &t.handler {
                    count += count_break_continue(&h.body.body);
                }
                if let Some(f) = &t.finalizer {
                    count += count_break_continue(&f.body);
                }
            }
            Statement::SwitchStatement(s) => {
                for case in &s.cases {
                    count += count_break_continue(&case.consequent);
                }
            }
            _ => {}
        }
    }
    count
}

fn count_break_continue_stmt(stmt: &Statement) -> usize {
    match stmt {
        Statement::BreakStatement(_) | Statement::ContinueStatement(_) => 1,
        Statement::BlockStatement(block) => count_break_continue(&block.body),
        _ => count_break_continue(std::slice::from_ref(stmt)),
    }
}

fn get_loop_body<'a>(node: AstKind<'a>) -> Option<(&'a Statement<'a>, oxc_span::Span)> {
    match node {
        AstKind::ForStatement(s) => Some((&s.body, s.span)),
        AstKind::ForInStatement(s) => Some((&s.body, s.span)),
        AstKind::ForOfStatement(s) => Some((&s.body, s.span)),
        AstKind::WhileStatement(s) => Some((&s.body, s.span)),
        AstKind::DoWhileStatement(s) => Some((&s.body, s.span)),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ForStatement,
            AstType::ForInStatement,
            AstType::ForOfStatement,
            AstType::WhileStatement,
            AstType::DoWhileStatement,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some((body, span)) = get_loop_body(node.kind()) else { return };
        let bc_count = count_break_continue_stmt(body);
        if bc_count >= 2 {
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Loop contains {bc_count} `break`/`continue` statements — consider refactoring."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_two_breaks() {
        let src = "for (const x of arr) {\n  if (a) break;\n  if (b) break;\n}";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_break_and_continue() {
        let src = "while (true) {\n  if (a) continue;\n  if (b) break;\n}";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_single_break() {
        let src = "for (const x of arr) {\n  if (a) break;\n  doWork();\n}";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_no_break() {
        let src = "for (const x of arr) {\n  doWork(x);\n}";
        assert!(run_on(src).is_empty());
    }
}

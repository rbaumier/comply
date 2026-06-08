//! no-unenclosed-multiline-block OXC backend — flag braceless if/for/while
//! with body on next line.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use oxc_span::GetSpan;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::IfStatement,
            AstType::ForStatement,
            AstType::ForInStatement,
            AstType::ForOfStatement,
            AstType::WhileStatement,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (keyword, stmt_start, body_span) = match node.kind() {
            AstKind::IfStatement(stmt) => {
                if matches!(&stmt.consequent, Statement::BlockStatement(_)) {
                    return;
                }
                ("if", stmt.span.start, stmt.consequent.span())
            }
            AstKind::ForStatement(stmt) => {
                if matches!(&stmt.body, Statement::BlockStatement(_)) {
                    return;
                }
                ("for", stmt.span.start, stmt.body.span())
            }
            AstKind::ForInStatement(stmt) => {
                if matches!(&stmt.body, Statement::BlockStatement(_)) {
                    return;
                }
                ("for", stmt.span.start, stmt.body.span())
            }
            AstKind::ForOfStatement(stmt) => {
                if matches!(&stmt.body, Statement::BlockStatement(_)) {
                    return;
                }
                ("for", stmt.span.start, stmt.body.span())
            }
            AstKind::WhileStatement(stmt) => {
                if matches!(&stmt.body, Statement::BlockStatement(_)) {
                    return;
                }
                ("while", stmt.span.start, stmt.body.span())
            }
            _ => return,
        };

        let (stmt_line, _) = byte_offset_to_line_col(ctx.source, stmt_start as usize);
        let (body_line, _) = byte_offset_to_line_col(ctx.source, body_span.start as usize);

        if body_line > stmt_line {
            let (line, column) = byte_offset_to_line_col(ctx.source, stmt_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{}` body is on the next line without curly braces \u{2014} wrap it in `{{}}`.",
                    keyword,
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn flags_multiline_if_without_braces() {
        let d = crate::rules::test_helpers::run_oxc_ts("if (condition)\n    doSomething();", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unenclosed-multiline-block");
    }


    #[test]
    fn flags_multiline_for_without_braces() {
        let d =
            crate::rules::test_helpers::run_oxc_ts("for (const x of items)\n    process(x);", &Check);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_braced_if() {
        let d =
            crate::rules::test_helpers::run_oxc_ts("if (condition) {\n    doSomething();\n}", &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn allows_single_line_if() {
        let d = crate::rules::test_helpers::run_oxc_ts("if (condition) doSomething();", &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn flags_while_without_braces() {
        let d = crate::rules::test_helpers::run_oxc_ts("while (running)\n    tick();", &Check);
        assert_eq!(d.len(), 1);
    }
}

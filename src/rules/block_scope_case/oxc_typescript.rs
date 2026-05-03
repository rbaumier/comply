use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchCase]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SwitchCase(case) = node.kind() else { return };

        for stmt in &case.consequent {
            match stmt {
                Statement::VariableDeclaration(decl)
                    if decl.kind.is_lexical() =>
                {
                    let span = decl.span;
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "block-scope-case".into(),
                        message: "Lexical declaration in `case` clause leaks into sibling cases — wrap the body in `{ ... }`.".into(),
                        severity: Severity::Warning,
                        span: Some((span.start as usize, (span.end - span.start) as usize)),
                    });
                    return;
                }
                Statement::ClassDeclaration(_) => {
                    let span = stmt.span();
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "block-scope-case".into(),
                        message: "Lexical declaration in `case` clause leaks into sibling cases — wrap the body in `{ ... }`.".into(),
                        severity: Severity::Warning,
                        span: Some((span.start as usize, (span.end - span.start) as usize)),
                    });
                    return;
                }
                _ => {}
            }
        }
    }
}

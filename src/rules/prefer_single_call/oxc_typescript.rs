use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Extract a "receiver.method" key from a call expression.
fn extract_call_key(expr: &Expression, source: &str) -> Option<String> {
    let Expression::CallExpression(call) = expr else { return None };
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    let prop = member.property.name.as_str();

    let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];

    if prop == "push" {
        return Some(format!("{obj_text}.push"));
    }

    if prop == "add" || prop == "remove" {
        if let Expression::StaticMemberExpression(inner) = &member.object {
            if inner.property.name.as_str() == "classList" {
                return Some(format!("{obj_text}.{prop}"));
            }
        }
    }

    None
}

fn scan_statements(stmts: &[Statement], source: &str, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let mut prev_key: Option<String> = None;

    for stmt in stmts {
        if let Statement::ExpressionStatement(expr_stmt) = stmt {
            if let Some(key) = extract_call_key(&expr_stmt.expression, source) {
                if let Some(ref pk) = prev_key {
                    if *pk == key {
                        let span = expr_stmt.expression.span();
                        let (line, column) = byte_offset_to_line_col(source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!("Combine consecutive `{key}()` calls into one."),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                prev_key = Some(key);
                continue;
            }
        }

        // Non-matching statement breaks the chain
        prev_key = None;

        // Recurse into blocks
        match stmt {
            Statement::BlockStatement(block) => {
                scan_statements(&block.body, source, ctx, diagnostics);
            }
            Statement::IfStatement(if_stmt) => {
                if let Statement::BlockStatement(block) = &if_stmt.consequent {
                    scan_statements(&block.body, source, ctx, diagnostics);
                }
                if let Some(ref alt) = if_stmt.alternate {
                    if let Statement::BlockStatement(block) = alt {
                        scan_statements(&block.body, source, ctx, diagnostics);
                    }
                }
            }
            Statement::ForStatement(for_stmt) => {
                if let Statement::BlockStatement(block) = &for_stmt.body {
                    scan_statements(&block.body, source, ctx, diagnostics);
                }
            }
            Statement::WhileStatement(while_stmt) => {
                if let Statement::BlockStatement(block) = &while_stmt.body {
                    scan_statements(&block.body, source, ctx, diagnostics);
                }
            }
            _ => {}
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let program = semantic.nodes().program();
        scan_statements(&program.body, ctx.source, ctx, &mut diagnostics);
        diagnostics
    }
}

//! react-prefer-use-reducer OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

fn is_use_state_call(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => match &call.callee {
            Expression::Identifier(id) => id.name == "useState",
            Expression::StaticMemberExpression(mem) => {
                if let Expression::Identifier(obj) = &mem.object {
                    obj.name == "React" && mem.property.name == "useState"
                } else {
                    false
                }
            }
            _ => false,
        },
        _ => false,
    }
}

/// Count `useState` calls in a list of statements without crossing nested
/// function boundaries.
fn count_use_state_in_stmts(stmts: &[Statement]) -> usize {
    let mut count = 0;
    for stmt in stmts {
        count += count_use_state_in_stmt(stmt);
    }
    count
}

fn count_use_state_in_stmt(stmt: &Statement) -> usize {
    match stmt {
        Statement::VariableDeclaration(decl) => {
            decl.declarations.iter().map(|d| {
                d.init.as_ref().map_or(0, |e| count_use_state_in_expr(e))
            }).sum()
        }
        Statement::ExpressionStatement(expr) => count_use_state_in_expr(&expr.expression),
        Statement::IfStatement(if_stmt) => {
            let mut c = count_use_state_in_expr(&if_stmt.test);
            if let Statement::BlockStatement(block) = &if_stmt.consequent {
                c += count_use_state_in_stmts(&block.body);
            }
            if let Some(alt) = &if_stmt.alternate {
                c += count_use_state_in_stmt(alt);
            }
            c
        }
        Statement::BlockStatement(block) => count_use_state_in_stmts(&block.body),
        Statement::ReturnStatement(ret) => {
            ret.argument.as_ref().map_or(0, |e| count_use_state_in_expr(e))
        }
        _ => 0,
    }
}

fn count_use_state_in_expr(expr: &Expression) -> usize {
    if is_use_state_call(expr) {
        return 1;
    }
    match expr {
        // Don't cross function boundaries.
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => 0,
        _ => 0,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useState"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Function(func) => {
                let Some(id) = &func.id else { return };
                let name = id.name.as_str();
                if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    return;
                }
                let Some(body) = &func.body else { return };
                self.check_body(&body.statements, name, func.span, ctx, diagnostics);
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                let parent_id = semantic.nodes().parent_id(node.id());
                if parent_id == node.id() {
                    return;
                }
                let parent = semantic.nodes().get_node(parent_id);
                let AstKind::VariableDeclarator(decl) = parent.kind() else {
                    return;
                };
                let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id else {
                    return;
                };
                let name = id.name.as_str();
                if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    return;
                }
                self.check_body(&arrow.body.statements, name, decl.span, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

impl Check {
    fn check_body(
        &self,
        stmts: &[Statement],
        name: &str,
        report_span: oxc_span::Span,
        ctx: &CheckCtx,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if stmts.is_empty() {
            return;
        }
        let max_state_calls =
            ctx.config.threshold("react-prefer-use-reducer", "max_state_calls", ctx.lang);
        let count = count_use_state_in_stmts(stmts);
        if count < max_state_calls {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, report_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Component `{name}` has {count} `useState` calls — consider `useReducer` for related state."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

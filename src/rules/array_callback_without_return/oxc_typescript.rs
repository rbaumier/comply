//! array-callback-without-return — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, FunctionBody, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

const ARRAY_METHODS: &[&str] = &[
    "map", "filter", "reduce", "find", "some", "every", "flatMap",
];

/// Check whether a function body contains a `return` statement (non-recursive
/// into nested functions).
fn body_has_return(body: &FunctionBody) -> bool {
    for stmt in &body.statements {
        if stmt_has_return(stmt) {
            return true;
        }
    }
    false
}

fn stmt_has_return(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(_) => true,
        // Don't descend into nested functions.
        Statement::FunctionDeclaration(_) => false,
        Statement::BlockStatement(block) => block.body.iter().any(|s| stmt_has_return(s)),
        Statement::IfStatement(if_stmt) => {
            stmt_has_return(&if_stmt.consequent)
                || if_stmt.alternate.as_ref().is_some_and(|a| stmt_has_return(a))
        }
        Statement::ForStatement(f) => stmt_has_return(&f.body),
        Statement::ForInStatement(f) => stmt_has_return(&f.body),
        Statement::ForOfStatement(f) => stmt_has_return(&f.body),
        Statement::WhileStatement(w) => stmt_has_return(&w.body),
        Statement::DoWhileStatement(d) => stmt_has_return(&d.body),
        Statement::SwitchStatement(s) => s
            .cases
            .iter()
            .any(|c| c.consequent.iter().any(|st| stmt_has_return(st))),
        Statement::TryStatement(t) => {
            t.block.body.iter().any(|s| stmt_has_return(s))
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.body.iter().any(|s| stmt_has_return(s)))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.body.iter().any(|s| stmt_has_return(s)))
        }
        Statement::LabeledStatement(l) => stmt_has_return(&l.body),
        Statement::WithStatement(w) => stmt_has_return(&w.body),
        _ => false,
    }
}

#[derive(Debug)]
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
        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Check if callee is a member expression calling an array method.
        let method_name = match &call.callee {
            Expression::StaticMemberExpression(mem) => &*mem.property.name,
            _ => return,
        };
        if !ARRAY_METHODS.contains(&method_name) {
            return;
        }

        // First argument must be an arrow function with a block body.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let arg_expr = match first_arg {
            Argument::ArrowFunctionExpression(arrow) => arrow,
            _ => return,
        };

        // Concise arrow `=> expr` — always has an implicit return.
        if arg_expr.expression {
            return;
        }

        if !body_has_return(&arg_expr.body) {
            let (line, col) =
                byte_offset_to_line_col(semantic.source_text(), arg_expr.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: "Array method callback uses block body `=> { ... }` without a `return` statement.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

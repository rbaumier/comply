//! ts-no-useless-constructor OxcCheck backend — flag constructors that are empty
//! or only call `super(...)` with the same arguments, and have no
//! accessibility modifiers, parameter properties, or decorators.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, MethodDefinitionKind, Statement,
};
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MethodDefinition]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::MethodDefinition(method) = node.kind() else {
            return;
        };
        if method.kind != MethodDefinitionKind::Constructor {
            return;
        }

        // Skip if constructor has accessibility modifier
        if method.accessibility.is_some() {
            return;
        }
        // Skip if override
        if method.r#override {
            return;
        }

        let func = &method.value;

        // Skip if any parameter has decorators, accessibility modifiers, or is a parameter property
        for param in &func.params.items {
            if !param.decorators.is_empty() {
                return;
            }
            if param.accessibility.is_some() {
                return;
            }
            if param.r#override {
                return;
            }
            if param.readonly {
                return;
            }
        }

        let Some(body) = &func.body else {
            return;
        };

        let stmts: Vec<&Statement> = body
            .statements
            .iter()
            .filter(|s| !matches!(s, Statement::EmptyStatement(_)))
            .collect();

        // Case 1: completely empty body
        if stmts.is_empty() {
            let (line, column) = byte_offset_to_line_col(ctx.source, method.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Useless constructor — remove it.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // Case 2: only `super(...)` call with same args passthrough
        if stmts.len() != 1 {
            return;
        }
        let Statement::ExpressionStatement(expr_stmt) = stmts[0] else {
            return;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            return;
        };
        let Expression::Super(_) = &call.callee else {
            return;
        };

        // Collect argument names (supporting spread)
        let arg_names: Vec<String> = call
            .arguments
            .iter()
            .filter_map(|arg| match arg {
                Argument::Identifier(ident) => Some(ident.name.to_string()),
                Argument::SpreadElement(spread) => {
                    if let Expression::Identifier(ident) = &spread.argument {
                        Some(format!("...{}", ident.name))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        // If any argument wasn't a simple identifier/spread-identifier, bail
        if arg_names.len() != call.arguments.len() {
            return;
        }

        // Handle rest parameter
        let mut formatted_params: Vec<String> = Vec::new();
        for param in &func.params.items {
            match &param.pattern {
                BindingPattern::BindingIdentifier(id) => {
                    formatted_params.push(id.name.to_string());
                }
                _ => return, // Complex pattern, bail
            }
        }
        if let Some(rest) = &func.params.rest {
            match &rest.rest.argument {
                BindingPattern::BindingIdentifier(id) => {
                    formatted_params.push(format!("...{}", id.name));
                }
                _ => return,
            }
        }

        if formatted_params == arg_names {
            let (line, column) = byte_offset_to_line_col(ctx.source, method.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Useless constructor — it only calls `super()` with the same arguments."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

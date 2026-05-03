//! no-useless-error-capture-stack-trace OXC backend — flag unnecessary
//! `Error.captureStackTrace(this, ClassName)` in Error subclass constructors.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const BUILTIN_ERRORS: &[&str] = &[
    "Error", "EvalError", "RangeError", "ReferenceError", "SyntaxError",
    "TypeError", "URIError", "AggregateError", "SuppressedError",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["captureStackTrace"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else {
            return;
        };

        // Check superclass is a builtin Error.
        let Some(ref super_class) = class.super_class else {
            return;
        };
        let super_name = match super_class {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !BUILTIN_ERRORS.contains(&super_name) {
            return;
        }

        // Get class name.
        let class_name = class
            .id
            .as_ref()
            .map(|id| id.name.as_str())
            .unwrap_or("");

        // Find the constructor.
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.kind != MethodDefinitionKind::Constructor {
                continue;
            }
            let Some(ref func_body) = method.value.body else {
                continue;
            };

            // Walk constructor body for `Error.captureStackTrace(this, ClassName)`.
            for stmt in &func_body.statements {
                let Statement::ExpressionStatement(expr_stmt) = stmt else {
                    continue;
                };
                let Expression::CallExpression(call) = &expr_stmt.expression else {
                    continue;
                };

                // Callee must be `Error.captureStackTrace`.
                let Expression::StaticMemberExpression(callee) = &call.callee else {
                    continue;
                };
                let Expression::Identifier(obj) = &callee.object else {
                    continue;
                };
                if obj.name.as_str() != "Error" {
                    continue;
                }
                if callee.property.name.as_str() != "captureStackTrace" {
                    continue;
                }

                // Check arguments: (this, ClassName) or (this, new.target) or (this, this.constructor).
                if call.arguments.len() != 2 {
                    continue;
                }

                let first_is_this = matches!(&call.arguments[0], Argument::ThisExpression(_));
                if !first_is_this {
                    continue;
                }

                let second_text = &ctx.source
                    [call.arguments[1].span().start as usize..call.arguments[1].span().end as usize];
                let is_class_ref = second_text == class_name
                    || second_text == "new.target"
                    || second_text == "this.constructor";

                if !is_class_ref {
                    continue;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-useless-error-capture-stack-trace".into(),
                    message: "Unnecessary `Error.captureStackTrace()` call. \
                              Built-in Error subclasses capture the stack \
                              trace automatically via `super()`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

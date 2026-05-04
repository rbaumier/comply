//! relative-url-style oxc backend — flag `new URL('./...', base)` where `./` is redundant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        // Constructor must be `URL`
        let Expression::Identifier(ident) = &new_expr.callee else { return };
        if ident.name.as_str() != "URL" {
            return;
        }

        // Must have two arguments (URL string + base)
        if new_expr.arguments.len() < 2 {
            return;
        }

        // First argument must be a string starting with './'
        let first_arg = &new_expr.arguments[0];
        let oxc_ast::ast::Argument::StringLiteral(lit) = first_arg else {
            // Also check template literals
            if let oxc_ast::ast::Argument::TemplateLiteral(tpl) = first_arg {
                if tpl.quasis.len() == 1 {
                    let raw = tpl.quasis[0].value.raw.as_str();
                    if raw.starts_with("./") {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Remove the `./` prefix from the relative URL in `new URL()`."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
            return;
        };

        if !lit.value.as_str().starts_with("./") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Remove the `./` prefix from the relative URL in `new URL()`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

//! ts-max-params OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn count_params(params: &oxc_ast::ast::FormalParameters) -> usize {
    params
        .items
        .iter()
        .filter(|p| {
            // Skip TS `this` parameter
            if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &p.pattern {
                if id.name.as_str() == "this" {
                    return false;
                }
            }
            true
        })
        .count()
}

fn func_name<'a>(func: &'a oxc_ast::ast::Function<'a>) -> &'a str {
    func.id.as_ref().map_or("<anonymous>", |id| id.name.as_str())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::ArrowFunctionExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let max_params = ctx.config.threshold("ts-max-params", "max", ctx.lang);

        let (count, name, span) = match node.kind() {
            AstKind::Function(func) => {
                (count_params(&func.params), func_name(func), func.span())
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                (count_params(&arrow.params), "<anonymous>", arrow.span())
            }
            _ => return,
        };

        if count > max_params {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Function `{name}` has {count} parameters (maximum allowed is {max_params})."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

//! ts-explicit-function-return-type OxcCheck backend — flag functions/methods
//! that lack an explicit return type annotation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::MethodDefinitionKind;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Function(func) => {
                // Skip constructors and setters — they don't take a return type.
                let parent = semantic.nodes().parent_node(node.id());
                if let AstKind::MethodDefinition(method) = parent.kind()
                    && (method.kind == MethodDefinitionKind::Set
                        || method.kind == MethodDefinitionKind::Constructor)
                    {
                        return;
                    }

                if func.return_type.is_some() {
                    return;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, func.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Missing return type on function.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if arrow.return_type.is_some() {
                    return;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, arrow.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Missing return type on function.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

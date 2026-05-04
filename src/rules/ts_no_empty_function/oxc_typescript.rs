//! ts-no-empty-function OxcCheck backend.
//!
//! Flag functions/methods with empty bodies that contain no comments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::FunctionBody;
use std::sync::Arc;

pub struct Check;

/// Returns true when the function body is empty (no statements, no directives)
/// and contains no comments in the source text between the braces.
fn is_empty_body(body: &FunctionBody, source: &str) -> bool {
    if !body.statements.is_empty() || !body.directives.is_empty() {
        return false;
    }
    // Check if there's a comment inside the body braces.
    let start = body.span.start as usize;
    let end = body.span.end as usize;
    if end > start && end <= source.len() {
        let inner = &source[start..end];
        // Strip outer braces
        let trimmed = inner.trim();
        if trimmed.len() > 2 {
            let content = trimmed[1..trimmed.len() - 1].trim();
            if content.starts_with("//") || content.starts_with("/*") {
                return false;
            }
        }
    }
    true
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (body_opt, span, is_method) = match node.kind() {
            AstKind::Function(func) => {
                // Check if this is a constructor with parameter properties
                // by looking at parent for MethodDefinition context.
                let parent = semantic.nodes().parent_node(node.id());
                let is_method = matches!(parent.kind(), AstKind::MethodDefinition(_));
                (func.body.as_ref(), func.span, is_method)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                (Some(&arrow.body), arrow.span, false)
            }
            _ => return,
        };

        let Some(body) = body_opt else { return };

        // Arrow functions with expression bodies (no block) are never empty.
        if matches!(node.kind(), AstKind::ArrowFunctionExpression(arrow) if arrow.expression) {
            return;
        }

        if !is_empty_body(body, ctx.source) {
            return;
        }

        // Skip constructors with parameter properties (accessibility modifiers).
        if is_method {
            if let AstKind::MethodDefinition(method) = semantic.nodes().parent_node(node.id()).kind() {
                if method.key.is_specific_id("constructor") {
                    if let AstKind::Function(func) = node.kind() {
                        for param in &func.params.items {
                            if param.accessibility.is_some() {
                                return;
                            }
                        }
                    }
                }
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected empty function.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

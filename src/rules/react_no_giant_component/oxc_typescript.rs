//! OXC backend for react-no-giant-component.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn subtree_has_jsx(node_span: oxc_span::Span, semantic: &oxc_semantic::Semantic) -> bool {
    for n in semantic.nodes().iter() {
        let s = n.kind().span();
        if s.start < node_span.start || s.end > node_span.end {
            continue;
        }
        match n.kind() {
            AstKind::JSXOpeningElement(_) | AstKind::JSXFragment(_) => return true,
            _ => {}
        }
    }
    false
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
        let (name, span, is_arrow) = match node.kind() {
            AstKind::Function(func) => {
                let Some(id) = &func.id else { return };
                let name = id.name.as_str();
                if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    return;
                }
                (name.to_string(), func.span, false)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                let parent = semantic.nodes().parent_node(node.id());
                let AstKind::VariableDeclarator(decl) = parent.kind() else {
                    return;
                };
                let BindingPattern::BindingIdentifier(id) = &decl.id else {
                    return;
                };
                let name = id.name.as_str();
                if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    return;
                }
                (name.to_string(), arrow.span, true)
            }
            _ => return,
        };

        if !subtree_has_jsx(span, semantic) {
            return;
        }

        let max = ctx.config.threshold("react-no-giant-component", "max", ctx.lang);

        let (start_line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
        let (end_line, _) = byte_offset_to_line_col(ctx.source, span.end as usize);
        let line_count = end_line - start_line + 1;

        if line_count <= max {
            return;
        }

        // For arrow functions, report at the variable declarator
        let report_span = if is_arrow {
            semantic.nodes().parent_node(node.id()).kind().span()
        } else {
            span
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, report_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Component `{name}` is {line_count} lines — break into smaller focused components."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

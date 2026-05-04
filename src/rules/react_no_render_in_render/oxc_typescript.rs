//! react-no-render-in-render OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const ALLOWED_RENDER_FNS: &[&str] = &[
    "renderToString",
    "renderToStaticMarkup",
    "renderToPipeableStream",
    "renderToReadableStream",
    "renderToStaticNodeStream",
    "renderToNodeStream",
    "renderHook",
];

fn is_render_call_name(name: &str) -> bool {
    if ALLOWED_RENDER_FNS.contains(&name) {
        return false;
    }
    if let Some(rest) = name.strip_prefix("render") {
        rest.starts_with(|c: char| c.is_ascii_uppercase())
    } else {
        false
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["render"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Get the callee name.
        let callee_name = match &call.callee {
            Expression::Identifier(id) => Some(id.name.as_str()),
            Expression::StaticMemberExpression(mem) => {
                if let Expression::Identifier(obj) = &mem.object {
                    if obj.name == "this" || obj.name == "self" {
                        Some(mem.property.name.as_str())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        let Some(name) = callee_name else { return };
        if !is_render_call_name(name) {
            return;
        }

        // Must be inside a JSX expression container.
        if !is_inside_jsx_expression(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Inline render function `{name}()` — extract to a component for proper reconciliation."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_inside_jsx_expression(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut current = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(current);
        if parent_id == current {
            return false;
        }
        current = parent_id;
        let parent = semantic.nodes().get_node(current);
        match parent.kind() {
            AstKind::JSXExpressionContainer(_) => return true,
            // Stop at function boundaries — the call must be directly
            // inside JSX, not inside a nested function that happens to be
            // in JSX.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => continue,
        }
    }
}

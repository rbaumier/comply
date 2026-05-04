//! OXC backend for prefer-top-level-await.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Skip CJS files
        let path_str = ctx.path.to_string_lossy();
        if path_str.ends_with(".cjs") {
            return;
        }

        // Check if this call is at the top level
        if !is_top_level_call(node, semantic) {
            return;
        }

        // Pattern 1: async IIFE — `(async () => { ... })()`
        if is_async_iife(call) {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-top-level-await".into(),
                message: "Prefer top-level await over an async IIFE.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // Pattern 2: top-level call to an async function defined at top level
        // Simple case: `main()` where `async function main() { ... }` exists
        let func_name = match &call.callee {
            Expression::Identifier(id) => Some(id.name.as_str()),
            // Handle `main().then(...)` — the callee is a member expression
            Expression::StaticMemberExpression(member) => {
                if member.property.name.as_str() == "then" {
                    if let Expression::CallExpression(inner_call) = &member.object {
                        if let Expression::Identifier(id) = &inner_call.callee {
                            Some(id.name.as_str())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        let Some(func_name) = func_name else {
            return;
        };

        // Check if there's a top-level async function declaration with this name
        if has_top_level_async_function(func_name, semantic, ctx.source) {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-top-level-await".into(),
                message: format!(
                    "Prefer top-level await over calling async function `{func_name}()`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn is_top_level_call(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent = nodes.get_node(parent_id);

    match parent.kind() {
        // Direct child of program: `expr;`
        AstKind::ExpressionStatement(_) => {
            let gp_id = nodes.parent_id(parent_id);
            if gp_id == parent_id {
                return true; // root
            }
            matches!(nodes.get_node(gp_id).kind(), AstKind::Program(_))
        }
        AstKind::Program(_) => true,
        _ => false,
    }
}

fn is_async_iife(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::ParenthesizedExpression(paren) => match &paren.expression {
            Expression::ArrowFunctionExpression(arrow) => arrow.r#async,
            Expression::FunctionExpression(func) => func.r#async,
            _ => false,
        },
        _ => false,
    }
}

fn has_top_level_async_function(
    name: &str,
    semantic: &oxc_semantic::Semantic,
    _source: &str,
) -> bool {
    let nodes = semantic.nodes();
    for node in nodes.iter() {
        let AstKind::Function(func) = node.kind() else {
            continue;
        };
        if !func.r#async {
            continue;
        }
        let Some(ref id) = func.id else {
            continue;
        };
        if id.name.as_str() != name {
            continue;
        }
        // Must be at program level (parent is program or export_statement)
        let parent_id = nodes.parent_id(node.id());
        if parent_id == node.id() {
            continue;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::Program(_) | AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => {
                return true;
            }
            _ => {}
        }
    }
    false
}

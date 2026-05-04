//! react-no-find-in-map-loop OxcCheck backend.

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

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["find", "filter"])
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

        // Must be a `.find(...)` or `.filter(...)` member call.
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };
        let method = mem.property.name.as_str();
        if method != "find" && method != "filter" {
            return;
        }

        // Check if it's inside a loop or .map() callback.
        let receiver_root = receiver_root_identifier(&mem.object);
        if !flagged_inside_loop_or_map(node, semantic, receiver_root.as_deref()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.find`/`.filter` inside a `.map` or loop — O(n\u{b2}). \
                      Build a `Map` once and look up inside the loop."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk up from `node` to determine if it's inside a loop or `.map()` callback.
fn flagged_inside_loop_or_map(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    receiver_root: Option<&str>,
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
            AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,
            AstKind::CallExpression(call) => {
                if is_map_call(call) {
                    // If the find/filter receiver root matches the map callback param,
                    // it's not the O(n^2) pattern.
                    let param = map_callback_param_name(call);
                    match (receiver_root, param.as_deref()) {
                        (Some(recv), Some(p)) if recv == p => {
                            // derived from current iteration item — keep looking up
                        }
                        _ => return true,
                    }
                }
            }
            _ => {}
        }
    }
}

fn is_map_call(call: &oxc_ast::ast::CallExpression) -> bool {
    if let Expression::StaticMemberExpression(mem) = &call.callee {
        mem.property.name == "map"
    } else {
        false
    }
}

/// Extract the first parameter name from a `.map(callback)`.
fn map_callback_param_name(call: &oxc_ast::ast::CallExpression) -> Option<String> {
    let first_arg = call.arguments.first()?;
    let expr = first_arg.to_expression();
    let params = match expr {
        Expression::ArrowFunctionExpression(arrow) => &arrow.params,
        Expression::FunctionExpression(func) => &func.params,
        _ => return None,
    };
    let first_param = params.items.first()?;
    let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &first_param.pattern else {
        return None;
    };
    Some(id.name.to_string())
}

/// Walk down the object chain of a member expression to find the leftmost
/// identifier. For `x.tags.find(...)` returns "x".
fn receiver_root_identifier(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(mem) => receiver_root_identifier(&mem.object),
        Expression::ComputedMemberExpression(mem) => receiver_root_identifier(&mem.object),
        Expression::CallExpression(call) => receiver_root_identifier(&call.callee),
        _ => None,
    }
}

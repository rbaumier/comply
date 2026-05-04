//! OXC backend for tanstack-query-no-query-in-effect.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useMutation",
    "useInfiniteQuery",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
];

const EFFECT_HOOKS: &[&str] = &["useEffect", "useLayoutEffect"];

pub struct Check;

fn callee_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

fn is_effect_callee(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => EFFECT_HOOKS.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "React"
                    && EFFECT_HOOKS.contains(&member.property.name.as_str())
            } else {
                false
            }
        }
        _ => false,
    }
}

fn is_inside_effect_callback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Check if this function is the callback argument of an effect hook.
                let parent = semantic.nodes().parent_node(ancestor.id());
                if let AstKind::CallExpression(call) = parent.kind() {
                    return is_effect_callee(&call.callee);
                }
                return false;
            }
            _ => {}
        }
    }
    false
}

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

        let Some(hook_name) = callee_name(&call.callee) else {
            return;
        };
        if !QUERY_HOOKS.contains(&hook_name) {
            return;
        }
        if !is_inside_effect_callback(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{hook_name}` inside `useEffect` — query hooks manage their own lifecycle."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

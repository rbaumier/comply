//! OXC backend for elysia-service-return-not-throw.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const LIFECYCLE_METHODS: &[&str] = &[
    "guard",
    "onError",
    "onRequest",
    "onTransform",
    "onParse",
    "onBeforeHandle",
    "beforeHandle",
    "onAfterHandle",
    "afterHandle",
    "derive",
    "resolve",
    "mapResponse",
    "onResponse",
    "trace",
    "state",
    "decorate",
    "macro",
];

fn imports_elysia(source: &str) -> bool {
    source.contains("from 'elysia'")
        || source.contains("from \"elysia\"")
        || source.contains("from 'elysia/")
        || source.contains("from \"elysia/")
        || source.contains("from '@elysiajs/")
        || source.contains("from \"@elysiajs/")
}

fn imports_frontend(source: &str) -> bool {
    source.contains("from 'react'")
        || source.contains("from \"react\"")
        || source.contains("from 'react/")
        || source.contains("from \"react/")
        || source.contains("from 'react-dom")
        || source.contains("from \"react-dom")
        || source.contains("from '@tanstack/")
        || source.contains("from \"@tanstack/")
        || source.contains("from 'vue'")
        || source.contains("from \"vue\"")
        || source.contains("from 'svelte")
        || source.contains("from \"svelte")
        || source.contains("from 'solid-js")
        || source.contains("from \"solid-js")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else {
            return;
        };

        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !imports_elysia(ctx.source) {
            return;
        }
        if imports_frontend(ctx.source) {
            return;
        }

        if is_inside_lifecycle_hook(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, throw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "elysia-service-return-not-throw".into(),
            message: "`throw` in Elysia code breaks typed error propagation — return `status(code, message)` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_inside_lifecycle_hook(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Check if this function is an argument to a lifecycle method call
                // Walk up through type wrappers
                let mut wrapper_id = parent_id;
                loop {
                    let gp_id = nodes.parent_id(wrapper_id);
                    if gp_id == wrapper_id {
                        break;
                    }
                    let gp = nodes.get_node(gp_id);
                    match gp.kind() {
                        AstKind::ParenthesizedExpression(_)
                        | AstKind::TSAsExpression(_)
                        | AstKind::TSSatisfiesExpression(_)
                        | AstKind::TSTypeAssertion(_)
                        | AstKind::TSNonNullExpression(_) => {
                            wrapper_id = gp_id;
                        }
                        _ => break,
                    }
                }

                // Check if wrapper's parent is an Argument in a CallExpression
                let arg_parent_id = nodes.parent_id(wrapper_id);
                if arg_parent_id == wrapper_id {
                    return false;
                }
                let arg_parent = nodes.get_node(arg_parent_id);

                // The function may be directly in a CallExpression's arguments
                if let AstKind::CallExpression(call) = arg_parent.kind() {
                    if let Some(method) = callee_method_name(call) {
                        if LIFECYCLE_METHODS.contains(&method) {
                            return true;
                        }
                    }
                }

                return false;
            }
            _ => {
                current_id = parent_id;
            }
        }
    }
    false
}

fn callee_method_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<&'a str> {
    match &call.callee {
        Expression::StaticMemberExpression(member) => {
            Some(member.property.name.as_str())
        }
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

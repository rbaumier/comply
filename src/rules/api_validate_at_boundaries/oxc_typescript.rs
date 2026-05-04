//! api-validate-at-boundaries OXC backend.
//!
//! Flags `.parse(...)` / `.safeParse(...)` calls in functions that don't
//! look like request handlers or middleware.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const HANDLER_KEYWORDS: &[&str] = &[
    "handler",
    "middleware",
    "controller",
    "endpoint",
    "resolver",
];

const HTTP_VERB_EXPORTS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

const REQUEST_PARAM_NAMES: &[&str] = &["req", "request", "ctx", "context"];

const ROUTE_VERBS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "head", "options", "all", "use",
];

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

        // Callee must be `.parse` or `.safeParse`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if method != "parse" && method != "safeParse" {
            return;
        }

        // Find enclosing function
        let Some((fn_name, fn_node)) = enclosing_function_info(node, semantic, ctx.source)
        else {
            // Top-level parse call — treat as boundary (module init). Skip.
            return;
        };

        if is_in_handler_context(fn_node, fn_name.as_deref(), semantic, ctx.source) {
            return;
        }

        let fn_label = fn_name.as_deref().unwrap_or("<anonymous>");
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{method}(...)` called inside `{fn_label}` — validate at the HTTP boundary only; internal callers should trust the typed contract."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn name_looks_like_handler(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if HANDLER_KEYWORDS.iter().any(|k| lower.contains(k)) {
        return true;
    }
    if HTTP_VERB_EXPORTS.contains(&name) {
        return true;
    }
    false
}

fn enclosing_function_info<'a>(
    node: &'a oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> Option<(Option<String>, &'a oxc_semantic::AstNode<'a>)> {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return None; // root
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::Function(func) => {
                let name = func.id.as_ref().map(|id| id.name.as_str().to_string());
                // If name is None, check if parent is a MethodDefinition
                if name.is_none() {
                    let gp_id = nodes.parent_id(parent_id);
                    if gp_id != parent_id {
                        let gp = nodes.get_node(gp_id);
                        if let AstKind::MethodDefinition(method) = gp.kind() {
                            let key_text = &source[method.key.span().start as usize..method.key.span().end as usize];
                            return Some((Some(key_text.to_string()), parent));
                        }
                    }
                }
                // If still None, check for VariableDeclarator parent (function expression)
                if name.is_none() {
                    let gp_id = nodes.parent_id(parent_id);
                    if gp_id != parent_id {
                        let gp = nodes.get_node(gp_id);
                        if let AstKind::VariableDeclarator(decl) = gp.kind()
                            && let BindingPattern::BindingIdentifier(id) = &decl.id {
                                return Some((Some(id.name.as_str().to_string()), parent));
                            }
                    }
                }
                return Some((name, parent));
            }
            AstKind::ArrowFunctionExpression(_) => {
                // Try to resolve assigned name via VariableDeclarator
                let mut name = None;
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id {
                    let gp = nodes.get_node(gp_id);
                    if let AstKind::VariableDeclarator(decl) = gp.kind()
                        && let BindingPattern::BindingIdentifier(id) = &decl.id {
                            name = Some(id.name.as_str().to_string());
                        }
                }
                return Some((name, parent));
            }
            _ => {
                current_id = parent_id;
            }
        }
    }
}

fn is_in_handler_context(
    fn_node: &oxc_semantic::AstNode,
    name: Option<&str>,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    if let Some(n) = name
        && name_looks_like_handler(n) {
            return true;
        }

    // Check parameters for request-like names/types
    match fn_node.kind() {
        AstKind::Function(func) => {
            if params_look_like_handler(&func.params, source) {
                return true;
            }
        }
        AstKind::ArrowFunctionExpression(arrow) => {
            if params_look_like_handler(&arrow.params, source) {
                return true;
            }
        }
        _ => {}
    }

    // Check if inline route callback
    if is_inline_route_callback(fn_node, semantic, source) {
        return true;
    }

    false
}

fn params_look_like_handler(params: &FormalParameters, source: &str) -> bool {
    for param in &params.items {
        if let BindingPattern::BindingIdentifier(id) = &param.pattern {
            let name = id.name.as_str();
            if REQUEST_PARAM_NAMES.contains(&name) {
                return true;
            }
        }
        // Check type annotation
        if let Some(type_ann) = &param.type_annotation {
            let type_text: &str = &source[type_ann.span().start as usize..type_ann.span().end as usize];
            if type_text.contains("Request") || type_text.contains("NextApiRequest") {
                return true;
            }
        }
    }
    false
}

fn is_inline_route_callback(
    fn_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    _source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(fn_node.id());
    if parent_id == fn_node.id() {
        return false;
    }
    let parent = nodes.get_node(parent_id);

    // For arrow/function expression: parent might be the CallExpression's arguments
    // We need to find the CallExpression ancestor
    let call_id = match parent.kind() {
        AstKind::CallExpression(_) => parent_id,
        _ => {
            // Try grandparent (might be wrapped in Argument)
            let gp_id = nodes.parent_id(parent_id);
            if gp_id == parent_id {
                return false;
            }
            let gp = nodes.get_node(gp_id);
            match gp.kind() {
                AstKind::CallExpression(_) => gp_id,
                _ => return false,
            }
        }
    };

    let call_node = nodes.get_node(call_id);
    let AstKind::CallExpression(call) = call_node.kind() else {
        return false;
    };

    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };

    let method = member.property.name.as_str();
    ROUTE_VERBS.contains(&method)
}

//! prefer-import-meta-properties OXC backend.
//!
//! Flags `fileURLToPath(import.meta.url)` and
//! `dirname(fileURLToPath(import.meta.url))` patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["import.meta"])
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
        let source = ctx.source;

        // 1. `path.dirname(fileURLToPath(import.meta.url))`
        if is_method_call_with_import_meta_url(call, "path", "dirname", source) {
            let (line, column) = byte_offset_to_line_col(source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-import-meta-properties".into(),
                message: "Use `import.meta.dirname` instead of `path.dirname(fileURLToPath(import.meta.url))`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // 2. `dirname(fileURLToPath(import.meta.url))`
        if is_call_to_with_import_meta_url_two_levels(call, "dirname", "fileURLToPath", source) {
            let (line, column) = byte_offset_to_line_col(source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-import-meta-properties".into(),
                message: "Use `import.meta.dirname` instead of `dirname(fileURLToPath(import.meta.url))`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // 3. `fileURLToPath(import.meta.url)` — skip if parent is dirname wrapper
        if is_call_to_with_import_meta_url(call, "fileURLToPath", source) {
            if has_dirname_wrapper_parent(node, semantic, source) {
                return;
            }
            let (line, column) = byte_offset_to_line_col(source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-import-meta-properties".into(),
                message: "Use `import.meta.filename` instead of `fileURLToPath(import.meta.url)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn is_import_meta_url(expr: &Expression, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    if member.property.name.as_str() != "url" {
        return false;
    }
    // The object should be `import.meta` — a MetaProperty
    let Expression::MetaProperty(_) = &member.object else {
        // Fallback: check source text
        let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];
        return obj_text == "import.meta";
    };
    true
}

fn single_call_arg<'a>(call: &'a CallExpression<'a>) -> Option<&'a Expression<'a>> {
    if call.arguments.len() != 1 {
        return None;
    }
    call.arguments[0].as_expression()
}

fn is_call_to_with_import_meta_url(
    call: &CallExpression,
    expected_callee: &str,
    source: &str,
) -> bool {
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    if id.name.as_str() != expected_callee {
        return false;
    }
    let Some(arg) = single_call_arg(call) else {
        return false;
    };
    is_import_meta_url(arg, source)
}

fn is_method_call_with_import_meta_url(
    call: &CallExpression,
    expected_object: &str,
    expected_method: &str,
    source: &str,
) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != expected_method {
        return false;
    }
    let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];
    if obj_text != expected_object {
        return false;
    }
    let Some(arg) = single_call_arg(call) else {
        return false;
    };
    // The single arg must be a call to `fileURLToPath(import.meta.url)`
    let Expression::CallExpression(inner_call) = arg else {
        return false;
    };
    is_call_to_with_import_meta_url(inner_call, "fileURLToPath", source)
}

fn is_call_to_with_import_meta_url_two_levels(
    call: &CallExpression,
    outer: &str,
    inner: &str,
    source: &str,
) -> bool {
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    if id.name.as_str() != outer {
        return false;
    }
    let Some(arg) = single_call_arg(call) else {
        return false;
    };
    let Expression::CallExpression(inner_call) = arg else {
        return false;
    };
    is_call_to_with_import_meta_url(inner_call, inner, source)
}

fn has_dirname_wrapper_parent(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    // Walk up: node -> argument position -> CallExpression
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent = nodes.get_node(parent_id);

    // The parent might be an Argument wrapper, or directly the CallExpression
    let call_node = match parent.kind() {
        AstKind::CallExpression(_) => parent,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            if gp_id == parent_id {
                return false;
            }
            let gp = nodes.get_node(gp_id);
            match gp.kind() {
                AstKind::CallExpression(_) => gp,
                _ => return false,
            }
        }
    };

    let AstKind::CallExpression(outer_call) = call_node.kind() else {
        return false;
    };

    match &outer_call.callee {
        Expression::Identifier(id) => id.name.as_str() == "dirname",
        Expression::StaticMemberExpression(member) => {
            let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];
            obj_text == "path" && member.property.name.as_str() == "dirname"
        }
        _ => false,
    }
}

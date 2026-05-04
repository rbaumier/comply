//! no-async-without-await OXC backend — flag `async` functions that contain
//! no `await` or `for await` in their own body.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_test_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("/tests/")
        || s.contains("\\tests\\")
}

/// Check if a function node has an explicit Promise return type annotation.
fn has_promise_return_type(
    source: &str,
    return_type: &Option<oxc_allocator::Box<oxc_ast::ast::TSTypeAnnotation>>,
) -> bool {
    let Some(rt) = return_type else { return false };
    let text = &source[rt.span.start as usize..rt.span.end as usize];
    text.contains("Promise<") || text.contains("PromiseLike<")
}

/// Find the nearest enclosing async function/arrow for a given node,
/// stopping at function boundaries. Returns the NodeId of the nearest
/// enclosing function/arrow.
fn nearest_function_id(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return Some(ancestor.id());
            }
            _ => {}
        }
    }
    None
}

/// Check if a method node or its class has decorators.
fn has_decorators(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(func_node.id()) {
        if let AstKind::MethodDefinition(method) = ancestor.kind() {
            if !method.decorators.is_empty() {
                return true;
            }
            // Check class decorators.
            for class_ancestor in semantic.nodes().ancestors(ancestor.id()) {
                if let AstKind::Class(class) = class_ancestor.kind() {
                    if !class.decorators.is_empty() {
                        return true;
                    }
                    break;
                }
            }
            return false;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return Vec::new();
        }

        // Collect node IDs of functions/arrows that contain an await or for-await.
        let mut has_await: std::collections::HashSet<oxc_semantic::NodeId> =
            std::collections::HashSet::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::AwaitExpression(_) => {
                    if let Some(func_id) = nearest_function_id(node, semantic) {
                        has_await.insert(func_id);
                    }
                }
                AstKind::ForOfStatement(for_of) if for_of.r#await => {
                    if let Some(func_id) = nearest_function_id(node, semantic) {
                        has_await.insert(func_id);
                    }
                }
                _ => {}
            }
        }

        // Now check all async functions/arrows.
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let (is_async, return_type, span, has_body) = match node.kind() {
                AstKind::Function(f) => (f.r#async, &f.return_type, f.span, f.body.is_some()),
                AstKind::ArrowFunctionExpression(f) => {
                    (f.r#async, &f.return_type, f.span, true)
                }
                _ => continue,
            };

            if !is_async || !has_body {
                continue;
            }

            if has_promise_return_type(ctx.source, return_type) {
                continue;
            }

            if has_decorators(node, semantic) {
                continue;
            }

            if has_await.contains(&node.id()) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "no-async-without-await".into(),
                message: "`async` function never awaits — drop the `async` keyword \
                          or add the `await` that justifies it."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

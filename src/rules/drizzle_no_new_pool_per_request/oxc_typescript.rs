//! drizzle-no-new-pool-per-request oxc backend — flag `new Pool(...)` / `drizzle(...)`
//! when they sit inside an exported function body.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        use oxc_ast::ast::Expression;

        match node.kind() {
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(ctor) = &new_expr.callee else {
                    return;
                };
                if ctor.name != "Pool" {
                    return;
                }
                if !inside_exported_function(node, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`new Pool()` in a handler body — move to module scope so connections are reused across requests.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                let Expression::Identifier(func) = &call.callee else {
                    return;
                };
                if func.name != "drizzle" {
                    return;
                }
                if !inside_exported_function(node, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`drizzle()` in a handler body — move to module scope so the client is reused across requests.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

/// Walk up the AST to find if this node is inside an exported function.
fn inside_exported_function(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut current_id = node.id();
    loop {
        let n = semantic.nodes().get_node(current_id);
        match n.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return is_function_exported(current_id, semantic);
            }
            _ => {}
        }
        let parent = semantic.nodes().parent_id(current_id);
        if parent == current_id {
            break;
        }
        current_id = parent;
    }
    false
}

fn is_function_exported(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut current_id = func_id;
    loop {
        let n = semantic.nodes().get_node(current_id);
        match n.kind() {
            AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => {
                return true;
            }
            // Stop at another function boundary — this function is nested.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) if current_id != func_id => {
                return false;
            }
            // For method definitions, check if the class is exported.
            AstKind::Class(_) => {
                // Continue walking to see if the class is exported.
            }
            _ => {}
        }
        let parent = semantic.nodes().parent_id(current_id);
        if parent == current_id {
            break;
        }
        current_id = parent;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_new_pool_in_handler() {
        let src = "export async function handler() { const pool = new Pool({}); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_drizzle_in_handler() {
        let src = "export const handler = async () => { const db = drizzle(pool); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_module_scope_pool() {
        let src = "const pool = new Pool({});\nconst db = drizzle(pool);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_pool_in_internal_factory() {
        let src = "function makePool() { const pool = new Pool({}); return pool; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_drizzle_in_internal_helper() {
        let src = "function makeDb(pool) { const db = drizzle(pool); return db; }";
        assert!(run(src).is_empty());
    }
}

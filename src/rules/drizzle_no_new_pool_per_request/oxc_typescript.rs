//! drizzle-no-new-pool-per-request oxc backend — flag `new Pool(...)` / `drizzle(...)`
//! when they sit inside an exported function body.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
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
        match node.kind() {
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(ctor) = &new_expr.callee else {
                    return;
                };
                if ctor.name != "Pool" {
                    return;
                }
                let Some(fn_id) = enclosing_exported_function(node, semantic) else {
                    return;
                };
                if construct_is_returned(node, semantic, fn_id) {
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
                let Some(fn_id) = enclosing_exported_function(node, semantic) else {
                    return;
                };
                if construct_is_returned(node, semantic, fn_id) {
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

/// Walk up the AST; if this node sits inside an exported function, return that
/// function's node id. Per-request handlers are exported, so this is the gate
/// before the factory check.
fn enclosing_exported_function(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    let mut current_id = node.id();
    loop {
        let n = semantic.nodes().get_node(current_id);
        match n.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return is_function_exported(current_id, semantic).then_some(current_id);
            }
            _ => {}
        }
        let parent = semantic.nodes().parent_id(current_id);
        if parent == current_id {
            break;
        }
        current_id = parent;
    }
    None
}

/// A function that hands its freshly-built client back to the caller is a
/// startup factory (e.g. `createDatabase`), not a per-request handler — the
/// caller owns the connection's lifetime. Skip when the constructed value is
/// part of the enclosing function's return.
fn construct_is_returned(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    fn_id: oxc_semantic::NodeId,
) -> bool {
    let construct_start = match node.kind() {
        AstKind::CallExpression(c) => c.span.start,
        AstKind::NewExpression(n) => n.span.start,
        _ => return false,
    };
    let name = assigned_name(node, semantic);
    for n in semantic.nodes().iter() {
        let AstKind::ReturnStatement(ret) = n.kind() else {
            continue;
        };
        if nearest_function(n.id(), semantic) != Some(fn_id) {
            continue;
        }
        if let Some(arg) = &ret.argument
            && expr_mentions(arg, name.as_deref(), construct_start)
        {
            return true;
        }
    }
    false
}

/// Name of the variable the construct is assigned to, if any
/// (`const database = drizzle(...)` → `database`).
fn assigned_name(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> Option<String> {
    let parent = semantic
        .nodes()
        .get_node(semantic.nodes().parent_id(node.id()));
    if let AstKind::VariableDeclarator(decl) = parent.kind()
        && let BindingPattern::BindingIdentifier(id) = &decl.id
    {
        return Some(id.name.to_string());
    }
    None
}

/// Nearest enclosing function node id, if any.
fn nearest_function(
    mut id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    loop {
        match semantic.nodes().get_node(id).kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return Some(id),
            _ => {}
        }
        let parent = semantic.nodes().parent_id(id);
        if parent == id {
            return None;
        }
        id = parent;
    }
}

/// Whether `expr` (a return argument) surfaces the constructed value — directly,
/// by its bound name, or as a property of a returned object literal.
fn expr_mentions(expr: &Expression, name: Option<&str>, construct_start: u32) -> bool {
    match expr {
        Expression::Identifier(id) => name == Some(id.name.as_str()),
        Expression::CallExpression(c) => c.span.start == construct_start,
        Expression::NewExpression(n) => n.span.start == construct_start,
        Expression::ObjectExpression(obj) => obj.properties.iter().any(|p| {
            matches!(p, oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop)
                if expr_mentions(&prop.value, name, construct_start))
        }),
        _ => false,
    }
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

    // Regression for #531: an exported startup factory hands the client back to
    // its caller, so it is not a per-request constructor.
    #[test]
    fn allows_exported_factory_returning_db_issue_531() {
        let src = "export function createDatabase(config) { const pgClient = postgres(config.url); const database = drizzle({ client: pgClient }); return { database }; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_exported_factory_direct_return_issue_531() {
        let src = "export function makeDb() { return drizzle(pool); }";
        assert!(run(src).is_empty());
    }
}

//! no-await-in-loop OXC backend — flag `await` inside a loop body, but
//! exempt recursive calls to the enclosing async function (deliberate
//! depth-first traversal).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use std::sync::Arc;

pub struct Check;

/// Extract the identifier name of the call target, if the awaited
/// expression is a direct call to an identifier or a `this.method` call.
/// `obj.method()` is NOT treated as self-recursion — only `this.method()` is.
fn awaited_callee_name<'a>(arg: &Expression<'a>) -> Option<&'a str> {
    let Expression::CallExpression(call) = arg else {
        return None;
    };
    match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            if matches!(member.object, Expression::ThisExpression(_)) {
                Some(member.property.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Walk ancestors of the `await` looking for a loop boundary. Stops at
/// function boundaries (a nested `async` function starts a fresh
/// context — its awaits are not "in" the outer loop). Returns the name
/// of the enclosing async function when a loop is found, so the caller
/// can compare against the awaited callee for recursion detection.
///
/// Return values:
///   - `Some(Some(name))` — inside a loop in a named async function
///   - `Some(None)` — inside a loop in an unnamed/arrow async function
///   - `None` — not inside a loop (or the enclosing function is reached first)
fn enclosing_loop_and_fn_name<'a>(
    node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<Option<&'a str>> {
    let nodes = semantic.nodes();
    let mut current_id = node_id;
    let mut saw_loop = false;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return None;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => {
                saw_loop = true;
            }
            AstKind::Function(func) => {
                if !saw_loop {
                    return None;
                }
                let name = func.id.as_ref().map(|id| id.name.as_str());
                return Some(name);
            }
            AstKind::ArrowFunctionExpression(_) => {
                if !saw_loop {
                    return None;
                }
                // Arrow functions are nameless at the syntax level, but
                // `const foo = async () => {}` is conventionally named
                // after the binding. Walk one more step to recover it.
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id
                    && let AstKind::VariableDeclarator(decl) = nodes.get_node(gp_id).kind()
                    && let BindingPattern::BindingIdentifier(id) = &decl.id
                {
                    return Some(Some(id.name.as_str()));
                }
                return Some(None);
            }
            _ => {}
        }
        current_id = parent_id;
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };

        let Some(enclosing_fn_name) = enclosing_loop_and_fn_name(node.id(), semantic) else {
            return;
        };

        // Recursion exception: if the awaited expression is a direct
        // call to the enclosing async function, skip — sequential
        // recursion is the only way to express depth-first traversal.
        if let (Some(fn_name), Some(callee)) =
            (enclosing_fn_name, awaited_callee_name(&await_expr.argument))
            && fn_name == callee
        {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Sequential `await` in a loop serializes independent work. \
                      If the iterations don't depend on each other, use \
                      `Promise.all(items.map(f))` instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

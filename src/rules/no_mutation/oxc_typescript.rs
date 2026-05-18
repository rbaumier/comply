//! no-mutation OXC backend — flag mutations on `const` bindings.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, UnaryOperator, VariableDeclarationKind};
use std::sync::Arc;

const MUTATING_ARRAY_METHODS: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

const OBJECT_MUTATOR_FUNCTIONS: &[&str] = &[
    "assign",
    "defineProperty",
    "defineProperties",
    "setPrototypeOf",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::AssignmentExpression,
            AstType::UpdateExpression,
            AstType::UnaryExpression,
            AstType::CallExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // obj.prop = x, obj.prop += x
            AstKind::AssignmentExpression(assign) => {
                // ref.current = ... (React useRef pattern)
                if is_current_target(&assign.left) {
                    return;
                }
                let Some(root) = root_name_of_target(&assign.left) else {
                    return;
                };
                if is_declared_as_const(semantic, root) {
                    report(diagnostics, ctx, assign.span.start, root, "Mutating property of");
                }
            }
            // obj.count++, --obj.count
            AstKind::UpdateExpression(update) => {
                let Some(root) = root_name_of_simple_target(&update.argument) else {
                    return;
                };
                if is_declared_as_const(semantic, root) {
                    report(diagnostics, ctx, update.span.start, root, "Mutating property of");
                }
            }
            // delete obj.prop
            AstKind::UnaryExpression(unary) => {
                if unary.operator != UnaryOperator::Delete {
                    return;
                }
                let Some(root) = root_name_of_expr(&unary.argument) else {
                    return;
                };
                if is_declared_as_const(semantic, root) {
                    report(diagnostics, ctx, unary.span.start, root, "Deleting property of");
                }
            }
            // arr.push(x), Object.assign(obj, ...)
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                let method = member.property.name.as_str();

                // Object.assign(target, ...)
                if OBJECT_MUTATOR_FUNCTIONS.contains(&method) {
                    if let Expression::Identifier(obj) = &member.object
                        && obj.name.as_str() == "Object"
                            && let Some(first_arg) = call.arguments.first() {
                                let root = match first_arg.as_expression() {
                                    Some(Expression::Identifier(ident)) => {
                                        Some(ident.name.as_str())
                                    }
                                    Some(expr) => root_name_of_expr(expr),
                                    None => None,
                                };
                                if let Some(root) = root
                                    && is_declared_as_const(semantic, root) {
                                        report(
                                            diagnostics,
                                            ctx,
                                            call.span.start,
                                            root,
                                            "Mutating",
                                        );
                                    }
                            }
                    return;
                }

                if !MUTATING_ARRAY_METHODS.contains(&method) {
                    return;
                }

                let root = match &member.object {
                    Expression::Identifier(ident) => Some(ident.name.as_str()),
                    expr => root_name_of_expr(expr),
                };
                let Some(root) = root else {
                    return;
                };

                // Skip `.push()` / `.unshift()` on a const local
                // accumulator inside a loop body — a common,
                // bounded, escape-free pattern. The structurally
                // correct alternative (`Result.all`) is missing from
                // better-result: tracking dmmulroy/better-result#32.
                //
                // Same exemption inside a `Result.gen(function*() { ... })`
                // block — the generator body is the canonical
                // accumulator site for sequencing `yield*` results,
                // and the spread alternative breaks short-circuiting
                // on the first error.
                if matches!(method, "push" | "unshift")
                    && matches!(&member.object, Expression::Identifier(_))
                    && (is_inside_loop_body(node, semantic)
                        || is_inside_result_gen(node, semantic))
                {
                    return;
                }

                if is_declared_as_const(semantic, root) {
                    report(
                        diagnostics,
                        ctx,
                        call.span.start,
                        root,
                        &format!("Calling `{method}()` on"),
                    );
                }
            }
            _ => {}
        }
    }
}

fn is_current_target(target: &AssignmentTarget) -> bool {
    match target {
        AssignmentTarget::StaticMemberExpression(member) => {
            member.property.name.as_str() == "current"
        }
        _ => false,
    }
}

/// Extract the root identifier name from an assignment target (must be member access).
fn root_name_of_target<'a>(target: &'a AssignmentTarget<'a>) -> Option<&'a str> {
    match target {
        // Plain identifier = reassignment, not property mutation
        AssignmentTarget::AssignmentTargetIdentifier(_) => None,
        AssignmentTarget::StaticMemberExpression(member) => root_name_of_expr(&member.object),
        AssignmentTarget::ComputedMemberExpression(member) => root_name_of_expr(&member.object),
        _ => None,
    }
}

fn root_name_of_simple_target<'a>(
    target: &'a oxc_ast::ast::SimpleAssignmentTarget<'a>,
) -> Option<&'a str> {
    match target {
        oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(m) => {
            root_name_of_expr(&m.object)
        }
        oxc_ast::ast::SimpleAssignmentTarget::ComputedMemberExpression(m) => {
            root_name_of_expr(&m.object)
        }
        _ => None,
    }
}

fn root_name_of_expr<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        Expression::StaticMemberExpression(member) => root_name_of_expr(&member.object),
        Expression::ComputedMemberExpression(member) => root_name_of_expr(&member.object),
        _ => None,
    }
}

/// Check if a name is declared as `const` in the current scope chain.
fn is_declared_as_const(semantic: &oxc_semantic::Semantic, name: &str) -> bool {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();

    for sym_id in scoping.symbol_ids() {
        if scoping.symbol_name(sym_id) != name {
            continue;
        }
        let decl_node_id = scoping.symbol_declaration(sym_id);
        // Walk up to find VariableDeclaration with const kind
        for kind in nodes.ancestor_kinds(decl_node_id) {
            match kind {
                AstKind::VariableDeclaration(decl) => {
                    return decl.kind == VariableDeclarationKind::Const;
                }
                AstKind::FormalParameter(_)
                | AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::Program(_) => {
                    return false;
                }
                _ => continue,
            }
        }
    }
    false
}

/// True if `node` sits inside a `for` / `for-of` / `for-in` / `while`
/// loop body, stopping at function boundaries. Used to recognise the
/// bounded local-accumulator pattern (`const items = []; for (...)
/// items.push(...);`) as a deliberate, escape-free mutation.
fn is_inside_loop_body(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `node` lives inside the generator function passed to
/// `Result.gen(function*() { ... })` (or an arrow form). The generator
/// body sequences `yield*` results into a local array — that's the
/// canonical accumulator site, and the spread alternative breaks
/// short-circuiting on the first error.
fn is_inside_result_gen(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(func) if func.generator => {
                let parent = nodes.parent_node(ancestor.id());
                if let AstKind::CallExpression(call) = parent.kind()
                    && is_result_gen_callee(&call.callee)
                {
                    return true;
                }
                return false;
            }
            AstKind::ArrowFunctionExpression(_) => {
                let parent = nodes.parent_node(ancestor.id());
                if let AstKind::CallExpression(call) = parent.kind()
                    && is_result_gen_callee(&call.callee)
                {
                    return true;
                }
                return false;
            }
            _ => {}
        }
    }
    false
}

fn is_result_gen_callee(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "Result" && member.property.name.as_str() == "gen"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn ignores_push_inside_result_gen_with_loop() {
        // Regression for rbaumier/comply#23 — canonical Result.gen accumulator.
        let src = r#"
            function mapResults(items, fn) {
                return Result.gen(function* () {
                    const mapped = [];
                    for (const item of items) {
                        mapped.push(yield* fn(item));
                    }
                    return mapped;
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_result_gen_without_loop() {
        // Regression for rbaumier/comply#23 — sequential yields inside Result.gen.
        let src = r#"
            function fetchAll() {
                return Result.gen(function* () {
                    const out = [];
                    out.push(yield* loadUser());
                    out.push(yield* loadOrders());
                    return out;
                });
            }
        "#;
        assert!(run(src).is_empty());
    }
}

fn report(diagnostics: &mut Vec<Diagnostic>, ctx: &CheckCtx, span_start: u32, root: &str, kind: &str) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "{kind} `{root}` (declared with `const`) — build a new value instead of mutating."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

//! pure-by-default OXC backend.
//!
//! Pub/sub store exception: if the module declares a root-scope `const`
//! variable initialised with `new Set(...)` (the subscriber list), the entire
//! file is treated as an intentional store and no violations are emitted.
//! This recognises the `useSyncExternalStore` / observer idiom where
//! module-level mutable state is the explicit architectural contract.
//!
//! Scheduler exception: if the module assigns the return value of a scheduling
//! primitive (`requestAnimationFrame` / `requestIdleCallback` / `setTimeout` /
//! `setInterval`) into a root-scope mutable binding, that binding is the frame
//! or timer handle of an intentional batching scheduler/coordinator. The
//! whole file is then treated as a scheduler whose module-level queue state is
//! the stated contract, so no violations are emitted.

use rustc_hash::FxHashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, BinaryOperator, Expression, Statement,
    UnaryOperator, VariableDeclarationKind,
};
use oxc_semantic::{NodeId, ReferenceFlags};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let root_scope = scoping.root_scope_id();

        if has_root_scope_const_set(nodes, scoping, root_scope) {
            return vec![];
        }

        if module_assigns_scheduler_handle(nodes, scoping, root_scope) {
            return vec![];
        }

        let mut diagnostics = Vec::new();
        let mut flagged: FxHashSet<NodeId> = FxHashSet::default();

        for symbol_id in scoping.symbol_ids() {
            if scoping.symbol_scope_id(symbol_id) != root_scope {
                continue;
            }
            if !is_let_or_var(nodes, scoping.symbol_declaration(symbol_id)) {
                continue;
            }
            if is_effectively_const_binding(scoping, symbol_id) {
                continue;
            }
            if is_write_once_lazy_init(nodes, scoping, symbol_id) {
                continue;
            }
            if is_scoped_context_bracket(nodes, scoping, symbol_id) {
                continue;
            }
            let var_name = scoping.symbol_name(symbol_id).to_string();

            for reference in scoping.get_resolved_references(symbol_id) {
                let Some((func_id, func_name)) =
                    enclosing_top_level_function(nodes, reference.node_id())
                else {
                    continue;
                };
                if is_pure_setter_for(nodes, func_id, func_name, &var_name) {
                    continue;
                }
                if !flagged.insert(func_id) {
                    continue;
                }
                let func_span = nodes.kind(func_id).span();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, func_span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Function `{func_name}` references mutable top-level state `{var_name}`."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

/// Returns true when the module contains a root-scope `const` variable
/// initialised with `new Set(...)`, which signals an intentional pub/sub or
/// observer store pattern.
fn has_root_scope_const_set(
    nodes: &oxc_semantic::AstNodes,
    scoping: &oxc_semantic::Scoping,
    root_scope: oxc_semantic::ScopeId,
) -> bool {
    scoping.symbol_ids().any(|symbol_id| {
        scoping.symbol_scope_id(symbol_id) == root_scope
            && is_const_new_set(nodes, scoping.symbol_declaration(symbol_id))
    })
}

/// True if the symbol's declaration is `const x = new Set(...)`.
fn is_const_new_set(nodes: &oxc_semantic::AstNodes, decl_id: NodeId) -> bool {
    // `ancestor_kinds` does not include the node itself, so prepend it.
    let mut init_is_set = false;
    for kind in
        std::iter::once(nodes.kind(decl_id)).chain(nodes.ancestor_kinds(decl_id))
    {
        match kind {
            AstKind::VariableDeclarator(declarator) => {
                init_is_set = declarator
                    .init
                    .as_ref()
                    .is_some_and(is_new_set_expression);
            }
            AstKind::VariableDeclaration(decl) => {
                return init_is_set
                    && matches!(decl.kind, VariableDeclarationKind::Const);
            }
            _ => {}
        }
    }
    false
}

fn is_new_set_expression(expr: &Expression) -> bool {
    let Expression::NewExpression(new_expr) = expr else {
        return false;
    };
    let Expression::Identifier(ident) = &new_expr.callee else {
        return false;
    };
    ident.name.as_str() == "Set"
}

/// Scheduling primitives whose return value is a frame/timer handle stored to
/// later cancel or track deferred work. Assigning one into a module-level
/// mutable binding is the signature of an intentional batching scheduler.
const SCHEDULER_PRIMITIVES: [&str; 4] = [
    "requestAnimationFrame",
    "requestIdleCallback",
    "setTimeout",
    "setInterval",
];

/// True when the module assigns the return value of a scheduling primitive into
/// a root-scope mutable (`let`/`var`) binding. That binding is the scheduler's
/// frame or timer handle, so the file is an intentional coordinator whose
/// module-level queue state is the stated contract. Covers both the declarator
/// form `let timer = setTimeout(...)` and the assignment form
/// `timer = requestAnimationFrame(...)`. A bare scheduling call whose handle is
/// discarded is not the coordinator signature and does not match.
fn module_assigns_scheduler_handle(
    nodes: &oxc_semantic::AstNodes,
    scoping: &oxc_semantic::Scoping,
    root_scope: oxc_semantic::ScopeId,
) -> bool {
    nodes.iter().any(|node| match node.kind() {
        AstKind::AssignmentExpression(assign) => {
            let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
                return false;
            };
            is_scheduler_primitive_call(&assign.right)
                && target.reference_id.get().is_some_and(|ref_id| {
                    scoping
                        .get_reference(ref_id)
                        .symbol_id()
                        .is_some_and(|symbol_id| {
                            scoping.symbol_scope_id(symbol_id) == root_scope
                                && is_let_or_var(nodes, scoping.symbol_declaration(symbol_id))
                        })
                })
        }
        AstKind::VariableDeclarator(declarator) => {
            declarator.kind != VariableDeclarationKind::Const
                && declarator
                    .init
                    .as_ref()
                    .is_some_and(is_scheduler_primitive_call)
                && declarator_is_root_scope(nodes, scoping, root_scope, node.id())
        }
        _ => false,
    })
}

/// True if `expr` is a direct call to one of [`SCHEDULER_PRIMITIVES`].
fn is_scheduler_primitive_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(ident) = &call.callee else {
        return false;
    };
    SCHEDULER_PRIMITIVES.contains(&ident.name.as_str())
}

/// True when `declarator_id`'s binding identifier resolves to a symbol declared
/// in the program's root scope.
fn declarator_is_root_scope(
    nodes: &oxc_semantic::AstNodes,
    scoping: &oxc_semantic::Scoping,
    root_scope: oxc_semantic::ScopeId,
    declarator_id: NodeId,
) -> bool {
    let AstKind::VariableDeclarator(declarator) = nodes.kind(declarator_id) else {
        return false;
    };
    let Some(ident) = declarator.id.get_binding_identifier() else {
        return false;
    };
    ident
        .symbol_id
        .get()
        .is_some_and(|symbol_id| scoping.symbol_scope_id(symbol_id) == root_scope)
}

/// True if a `let`/`var` binding is never reassigned after its declarator, so
/// it is constant in practice. A function reading such a binding is pure with
/// respect to it. `Scoping::symbol_is_mutated` reports whether any resolved
/// reference writes to the symbol; the initial declarator is not a reference,
/// so an alias like `let isArray = Array.isArray` reports `false`.
fn is_effectively_const_binding(
    scoping: &oxc_semantic::Scoping,
    symbol_id: oxc_semantic::SymbolId,
) -> bool {
    !scoping.symbol_is_mutated(symbol_id)
}

/// True if the binding is a write-once lazy-init cache: it starts unset (no
/// initializer or `= undefined`) and every write to it is a lazy-init write —
/// either a `??=` nullish assignment, or a plain `=` assignment guarded by an
/// enclosing `if (binding === undefined)` / `== null` / `typeof binding ===
/// 'undefined'` check of the same binding. The value is set exactly once on
/// first access and never changes thereafter, so a function reading or seeding
/// it returns the same value on every call and is idempotent. Any other write
/// (unconditional plain `=`, compound, or a guard testing a different binding or
/// the inverse direction) would mutate the value past the first set, so it
/// disqualifies the binding and keeps it flagged.
fn is_write_once_lazy_init(
    nodes: &oxc_semantic::AstNodes,
    scoping: &oxc_semantic::Scoping,
    symbol_id: oxc_semantic::SymbolId,
) -> bool {
    if !declaration_starts_unset(nodes, scoping.symbol_declaration(symbol_id)) {
        return false;
    }
    let mut saw_write = false;
    for reference in scoping.get_resolved_references(symbol_id) {
        if !reference.flags().contains(ReferenceFlags::Write) {
            continue;
        }
        saw_write = true;
        if !write_is_nullish_assignment(nodes, reference.node_id())
            && !write_is_if_undefined_guarded_assignment(nodes, reference.node_id())
        {
            return false;
        }
    }
    saw_write
}

/// True if the binding's declarator has no initializer or initialises to
/// `undefined`, i.e. it starts in the unset state a lazy cache fills on first use.
fn declaration_starts_unset(
    nodes: &oxc_semantic::AstNodes,
    decl_id: NodeId,
) -> bool {
    for kind in
        std::iter::once(nodes.kind(decl_id)).chain(nodes.ancestor_kinds(decl_id))
    {
        if let AstKind::VariableDeclarator(declarator) = kind {
            return match &declarator.init {
                None => true,
                Some(Expression::Identifier(ident)) => {
                    ident.name.as_str() == "undefined"
                }
                Some(_) => false,
            };
        }
    }
    false
}

/// True when the write at `ref_node_id` is the left target of a `??=`
/// (nullish-coalescing) assignment.
fn write_is_nullish_assignment(
    nodes: &oxc_semantic::AstNodes,
    ref_node_id: NodeId,
) -> bool {
    matches!(
        nodes.ancestor_kinds(ref_node_id).next(),
        Some(AstKind::AssignmentExpression(assign))
            if assign.operator == AssignmentOperator::LogicalNullish
    )
}

/// True when the write at `ref_node_id` is a plain `=` assignment that sits in
/// the consequent of an `if (binding === undefined)` guard testing the SAME
/// binding for being unset. This is the `if (cache === undefined) { cache =
/// compute(); }` lazy-init idiom, structurally equivalent to `cache ??=
/// compute()`: the binding is filled exactly once on first access. Only the
/// unset-direction tests (`===`/`==` against `undefined`/`null`, or `typeof
/// binding === 'undefined'`) in the consequent qualify; the inverse (`!==`/`!=`)
/// or a guard on a different binding does not, since those would write when the
/// value is already set.
fn write_is_if_undefined_guarded_assignment(
    nodes: &oxc_semantic::AstNodes,
    ref_node_id: NodeId,
) -> bool {
    let mut kinds = nodes.ancestor_kinds(ref_node_id);

    // The write target must be a plain `=` assignment to a bare identifier.
    let Some(AstKind::AssignmentExpression(assign)) = kinds.next() else {
        return false;
    };
    if assign.operator != AssignmentOperator::Assign {
        return false;
    }
    let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
        return false;
    };

    // Walk up through the statement wrappers to the nearest enclosing
    // `IfStatement`, tracking the child we entered it through so we can require
    // the assignment is in the consequent (then-branch), not the alternate.
    let mut child_span = assign.span();
    for kind in kinds {
        match kind {
            AstKind::IfStatement(if_stmt) => {
                if if_stmt.consequent.span() != child_span {
                    return false;
                }
                return test_is_unset_check(&if_stmt.test, target.name.as_str());
            }
            AstKind::ExpressionStatement(_) | AstKind::BlockStatement(_) => {
                child_span = kind.span();
            }
            _ => return false,
        }
    }
    false
}

/// True when `test` is an "is-unset" check of the binding named `name`:
/// `name === undefined`, `name == null`, `name === null`, or
/// `typeof name === 'undefined'` (either operand order). Only the equality
/// direction is accepted; the inverse (`!==`/`!=`) is not, as it would gate the
/// write on the value already being set.
fn test_is_unset_check(test: &Expression, name: &str) -> bool {
    let Expression::BinaryExpression(bin) = test else {
        return false;
    };
    if !matches!(
        bin.operator,
        BinaryOperator::StrictEquality | BinaryOperator::Equality
    ) {
        return false;
    }
    // `binding === undefined` / `binding == null` / `binding === null`.
    if (operand_is_binding(&bin.left, name) && operand_is_unset_literal(&bin.right))
        || (operand_is_binding(&bin.right, name)
            && operand_is_unset_literal(&bin.left))
    {
        return true;
    }
    // `typeof binding === 'undefined'`.
    (typeof_targets_binding(&bin.left, name)
        && operand_is_undefined_string(&bin.right))
        || (typeof_targets_binding(&bin.right, name)
            && operand_is_undefined_string(&bin.left))
}

/// True when `expr` is a plain identifier reference named `name`.
fn operand_is_binding(expr: &Expression, name: &str) -> bool {
    matches!(expr, Expression::Identifier(ident) if ident.name.as_str() == name)
}

/// True when `expr` is the `undefined` identifier or a `null` literal — the
/// values a lazy cache holds before its first write.
fn operand_is_unset_literal(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(ident) => ident.name.as_str() == "undefined",
        Expression::NullLiteral(_) => true,
        _ => false,
    }
}

/// True when `expr` is `typeof name`, i.e. a `typeof` unary applied to the
/// identifier `name`.
fn typeof_targets_binding(expr: &Expression, name: &str) -> bool {
    let Expression::UnaryExpression(unary) = expr else {
        return false;
    };
    unary.operator == UnaryOperator::Typeof
        && operand_is_binding(&unary.argument, name)
}

/// True when `expr` is the string literal `'undefined'`.
fn operand_is_undefined_string(expr: &Expression) -> bool {
    matches!(expr, Expression::StringLiteral(lit) if lit.value == "undefined")
}

/// True when the binding is a scoped-context bracket: a value that is saved,
/// temporarily swapped, and restored in a `finally` block (or popped back from a
/// caller-supplied parameter). This is the push/pop "current observer" pointer
/// of reactive libraries — temporarily impure by contract, since threading it
/// through every call as a parameter would defeat the implicit dependency
/// tracking the binding exists to provide.
///
/// The binding qualifies iff at least one write restores it inside a `finally`
/// block AND every write is accounted for by the bracket shape:
/// - a RESTORE write — a plain `=` whose RHS is a parameter of the enclosing
///   function or a `const saved = binding` local of it — is always accounted; a
///   RESTORE inside a `finally` additionally proves the bracket;
/// - a MODIFY write — any other plain `=` RHS — is accounted only when its own
///   enclosing function restores the binding in a `finally` block.
///
/// A single unaccounted write (a compound op, a non-identifier target, a plain
/// reassignment with no bracketing `finally`) disqualifies the whole binding, so
/// genuine shared mutable state stays flagged.
fn is_scoped_context_bracket(
    nodes: &oxc_semantic::AstNodes,
    scoping: &oxc_semantic::Scoping,
    symbol_id: oxc_semantic::SymbolId,
) -> bool {
    let var_name = scoping.symbol_name(symbol_id);
    let mut saw_finally_restore = false;
    for reference in scoping.get_resolved_references(symbol_id) {
        if !reference.flags().contains(ReferenceFlags::Write) {
            continue;
        }
        let ref_node_id = reference.node_id();
        let Some(AstKind::AssignmentExpression(assign)) =
            nodes.ancestor_kinds(ref_node_id).next()
        else {
            return false;
        };
        if assign.operator != AssignmentOperator::Assign {
            return false;
        }
        let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
            return false;
        };
        if target.name.as_str() != var_name {
            return false;
        }
        let Some((func_id, _)) = enclosing_top_level_function(nodes, ref_node_id) else {
            return false;
        };
        if write_rhs_is_restore(nodes, &assign.right, func_id, var_name) {
            saw_finally_restore |= write_is_in_finally(nodes, ref_node_id, func_id);
        } else if !function_has_finally_restore(nodes, func_id, var_name) {
            return false;
        }
    }
    saw_finally_restore
}

/// True when the assignment RHS restores a previously-saved context value: it
/// names a parameter of the enclosing function (the popped value) or a `const
/// saved = var_name` local declared in that function.
fn write_rhs_is_restore(
    nodes: &oxc_semantic::AstNodes,
    rhs: &Expression,
    func_id: NodeId,
    var_name: &str,
) -> bool {
    let Expression::Identifier(id) = rhs else {
        return false;
    };
    let candidate = id.name.as_str();
    function_param_named(nodes, func_id, candidate)
        || function_has_saved_local(nodes, func_id, candidate, var_name)
}

/// True when the function `func_id` has a simple parameter named `name`.
fn function_param_named(
    nodes: &oxc_semantic::AstNodes,
    func_id: NodeId,
    name: &str,
) -> bool {
    let AstKind::Function(func) = nodes.kind(func_id) else {
        return false;
    };
    func.params.items.iter().any(|param| {
        param
            .pattern
            .get_binding_identifier()
            .is_some_and(|id| id.name.as_str() == name)
    })
}

/// True when the function `func_id` declares `const <local_name> = <var_name>`
/// directly in its body — the saved copy of the context value the bracket later
/// restores.
fn function_has_saved_local(
    nodes: &oxc_semantic::AstNodes,
    func_id: NodeId,
    local_name: &str,
    var_name: &str,
) -> bool {
    let AstKind::Function(func) = nodes.kind(func_id) else {
        return false;
    };
    let Some(body) = func.body.as_ref() else {
        return false;
    };
    body.statements.iter().any(|stmt| {
        let Statement::VariableDeclaration(decl) = stmt else {
            return false;
        };
        decl.kind == VariableDeclarationKind::Const
            && decl.declarations.iter().any(|declarator| {
                declarator
                    .id
                    .get_binding_identifier()
                    .is_some_and(|id| id.name.as_str() == local_name)
                    && matches!(
                        &declarator.init,
                        Some(Expression::Identifier(init))
                            if init.name.as_str() == var_name
                    )
            })
    })
}

/// True when the function `func_id` has a direct `try` statement whose
/// `finalizer` block restores `var_name` with a plain `var_name = <identifier>;`
/// assignment.
fn function_has_finally_restore(
    nodes: &oxc_semantic::AstNodes,
    func_id: NodeId,
    var_name: &str,
) -> bool {
    let AstKind::Function(func) = nodes.kind(func_id) else {
        return false;
    };
    let Some(body) = func.body.as_ref() else {
        return false;
    };
    body.statements.iter().any(|stmt| {
        let Statement::TryStatement(try_stmt) = stmt else {
            return false;
        };
        try_stmt.finalizer.as_ref().is_some_and(|finalizer| {
            finalizer
                .body
                .iter()
                .any(|s| stmt_is_identifier_restore_to(s, var_name))
        })
    })
}

/// True if `stmt` is a plain `var_name = <identifier>;` assignment — the shape
/// of restoring a saved context value.
fn stmt_is_identifier_restore_to(stmt: &Statement, var_name: &str) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
        return false;
    };
    if assign.operator != AssignmentOperator::Assign {
        return false;
    }
    let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
        return false;
    };
    target.name.as_str() == var_name && matches!(assign.right, Expression::Identifier(_))
}

/// True when the write at `ref_node_id` lies inside the `finalizer` block of a
/// `try` statement within the enclosing function `func_id`. The finalizer is
/// matched by identity: its span must equal that of one of the write's
/// block-statement ancestors below the function.
fn write_is_in_finally(
    nodes: &oxc_semantic::AstNodes,
    ref_node_id: NodeId,
    func_id: NodeId,
) -> bool {
    let mut block_spans = Vec::new();
    for (kind, node_id) in nodes
        .ancestor_kinds(ref_node_id)
        .zip(nodes.ancestor_ids(ref_node_id))
    {
        if node_id == func_id {
            break;
        }
        match kind {
            AstKind::BlockStatement(block) => block_spans.push(block.span),
            AstKind::TryStatement(try_stmt) => {
                if let Some(finalizer) = try_stmt.finalizer.as_ref()
                    && block_spans.contains(&finalizer.span)
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// True if the symbol's declaration sits inside a `let` or `var`
/// `VariableDeclaration`.
fn is_let_or_var(nodes: &oxc_semantic::AstNodes, decl_id: NodeId) -> bool {
    for kind in nodes.ancestor_kinds(decl_id) {
        if let AstKind::VariableDeclaration(decl) = kind {
            return matches!(
                decl.kind,
                VariableDeclarationKind::Let | VariableDeclarationKind::Var
            );
        }
    }
    false
}

/// Walk up from `start` until we hit a `Function` declaration whose
/// nearest enclosing scope is the program. Returns `(node_id, name)`.
fn enclosing_top_level_function<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    start: NodeId,
) -> Option<(NodeId, &'a str)> {
    let mut last_function: Option<(NodeId, &'a str)> = None;
    for (kind, node_id) in nodes.ancestor_kinds(start).zip(nodes.ancestor_ids(start)) {
        match kind {
            AstKind::Function(func) => {
                if let Some(ident) = &func.id {
                    last_function = Some((node_id, ident.name.as_str()));
                }
            }
            AstKind::ArrowFunctionExpression(_) => {
                return None;
            }
            AstKind::Program(_) => {
                return last_function;
            }
            _ => {}
        }
    }
    None
}

/// True when `func_id` is a deliberate public setter for `var_name`: its body
/// does nothing but assign to that module-level binding, and its name follows
/// the setter convention. Such a function is impure by design — mutating the
/// config is its sole stated purpose — so it is not a violation.
///
/// Both signals are required for precision:
/// - structural: the body is a single `var_name = <expr>` assignment, optionally
///   followed by a bare `return;` (void return);
/// - naming: the name starts with `set`, or `var_name` appears within the name.
///
/// A function that also reads the binding for a computation, returns a value, or
/// runs any other statement fails the structural check and stays flagged.
fn is_pure_setter_for(
    nodes: &oxc_semantic::AstNodes,
    func_id: NodeId,
    func_name: &str,
    var_name: &str,
) -> bool {
    let AstKind::Function(func) = nodes.kind(func_id) else {
        return false;
    };
    let Some(body) = func.body.as_ref() else {
        return false;
    };
    if !body_is_sole_assignment_to(&body.statements, var_name) {
        return false;
    }
    name_follows_setter_convention(func_name, var_name)
}

/// True when `statements` is exactly a single assignment `var_name = <expr>`
/// (plain `=`, never a compound op), optionally followed by a bare `return;`.
fn body_is_sole_assignment_to(statements: &[Statement], var_name: &str) -> bool {
    let mut iter = statements.iter();
    let Some(first) = iter.next() else {
        return false;
    };
    if !is_plain_assignment_to(first, var_name) {
        return false;
    }
    match iter.next() {
        None => true,
        Some(second) => is_bare_return(second) && iter.next().is_none(),
    }
}

/// True if `stmt` is `var_name = <expr>;` with the plain `=` operator.
fn is_plain_assignment_to(stmt: &Statement, var_name: &str) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
        return false;
    };
    if assign.operator != AssignmentOperator::Assign {
        return false;
    }
    let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
        return false;
    };
    target.name.as_str() == var_name
}

/// True if `stmt` is `return;` with no returned value.
fn is_bare_return(stmt: &Statement) -> bool {
    matches!(stmt, Statement::ReturnStatement(ret) if ret.argument.is_none())
}

/// True if the function name follows the setter convention for `var_name`:
/// it starts with `set`, or `var_name` appears within it (case-insensitive).
fn name_follows_setter_convention(func_name: &str, var_name: &str) -> bool {
    func_name.starts_with("set")
        || func_name.to_ascii_lowercase().contains(&var_name.to_ascii_lowercase())
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    /// Run through the production applicability gate, so `skip_in_test_dir`
    /// suppresses the rule for test-file paths.
    fn run_gated(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, source, path)
    }

    #[test]
    fn flags_function_using_top_level_let() {
        let src = "let counter = 0;\nfunction increment() { counter += 1; }\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
        assert!(d[0].message.contains("counter"));
    }

    #[test]
    fn allows_function_without_top_level_state() {
        let src = "const MAX = 100;\nfunction add(a: number, b: number) { return a + b; }\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_var_at_top_level() {
        let src = "var state = {};\nfunction reset() { state = {}; }\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("reset"));
    }

    // Regression for #577 — pub/sub store with useSyncExternalStore pattern.
    // The module-level `const subscribers = new Set<() => void>()` signals
    // intentional mutable state; no violation should be emitted.
    #[test]
    fn no_fp_on_pubsub_store_with_const_set() {
        let src = r#"
let titleByPathname = new Map<string, string>();
const subscribers = new Set<() => void>();

export function setLiveRouteTitle(pathname: string, title: string) {
    titleByPathname.set(pathname, title);
    subscribers.forEach((cb) => cb());
}

export function clearLiveRouteTitle(pathname: string) {
    titleByPathname.delete(pathname);
    subscribers.forEach((cb) => cb());
}

export function getLiveRouteTitlesSnapshot() {
    return titleByPathname;
}

export function resetLiveRouteTitlesForTests() {
    titleByPathname = new Map();
}
"#;
        assert!(run(src).is_empty(), "pub/sub store functions must not be flagged");
    }

    #[test]
    fn no_fp_on_module_with_exported_const_set_subscriber() {
        // Even when the Set is exported, the pattern is still a pub/sub store.
        let src = r#"
let state = 0;
export const listeners = new Set<() => void>();
export function setState(n: number) { state = n; listeners.forEach(l => l()); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_mutable_state_without_const_set() {
        // A plain mutable module-level variable without a subscriber Set is
        // still a violation.
        let src = r#"
let counter = 0;
export function increment() { counter += 1; }
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
    }

    // Regression for #1890 — immer-style `export let` aliases that are
    // declared once and never reassigned are effectively constant; a function
    // reading them is pure with respect to that binding.
    #[test]
    fn no_fp_on_never_reassigned_root_let() {
        let src = r#"
export let isArray = Array.isArray
export let isObjectish = (target: any) => typeof target === "object"

export function isDraftable(value: any): boolean {
    if (!value) return false
    return isObjectish(value) || isArray(value)
}
"#;
        assert!(
            run(src).is_empty(),
            "never-reassigned root-scope let must not be flagged"
        );
    }

    #[test]
    fn flags_reassigned_root_let_read_by_function() {
        // A counter that IS reassigned (`counter = ...`) and read inside a
        // function is genuine mutable state and must still be flagged.
        let src = r#"
let counter = 0;
export function bump() {
    counter = counter + 1;
    return counter;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bump"));
        assert!(d[0].message.contains("counter"));
    }

    // Regression for #2241 — a deliberate public config setter whose entire
    // body is a single assignment to the module-level `let` is impure by
    // design, not an accidental side effect.
    #[test]
    fn no_fp_on_public_config_setter() {
        let src = r#"
export let mapStoreSuffix = 'Store'
export function setMapStoreSuffix(suffix: string): void {
    mapStoreSuffix = suffix
}
"#;
        assert!(
            run(src).is_empty(),
            "a pure setter (single module-let assignment) must not be flagged"
        );
    }

    #[test]
    fn no_fp_on_setter_with_trailing_void_return() {
        // A trailing bare `return;` is still a void setter.
        let src = r#"
let theme = 'light'
export function setTheme(next: string): void {
    theme = next
    return
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_setter_named_after_the_variable() {
        // Name does not start with `set` but contains the variable name.
        let src = r#"
let locale = 'en'
export function updateLocale(next: string) {
    locale = next
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_setter_that_does_extra_work() {
        // Body is more than the assignment: it also reads/computes, so it is
        // not a pure setter and stays flagged.
        let src = r#"
let count = 0
export function setCount(n: number) {
    count = n
    console.log(count)
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setCount"));
    }

    #[test]
    fn flags_setter_with_compound_assignment() {
        // A compound `+=` reads the previous value, so it is not a plain
        // setter even with a setter-style name.
        let src = r#"
let total = 0
export function setTotal(n: number) {
    total += n
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setTotal"));
    }

    #[test]
    fn flags_function_that_only_reads_mutable_state() {
        // Reads a mutable top-level `let` for a computation — no assignment at
        // all, so it is not a setter and must stay flagged.
        let src = r#"
let counter = 1
export function setCounter(n: number) {
    counter = n
}
export function compute() {
    return counter * 2
}
"#;
        let d = run(src);
        // `setCounter` is a pure setter (exempt); `compute` reads the mutable
        // state for a computation and must stay flagged.
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("compute"));
        assert!(d[0].message.contains("counter"));
    }

    #[test]
    fn flags_single_assignment_without_setter_name() {
        // Body is a single assignment but the name neither starts with `set`
        // nor mentions the variable — the naming signal is missing, so the
        // structural-only match is rejected.
        let src = r#"
let flag = false
export function toggle(v: boolean) {
    flag = v
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toggle"));
    }

    // Regression for #5498 — an animation-frame scheduler/coordinator
    // (infernojs/inferno `animationCoordinator`). A root-scope mutable binding
    // assigned the return value of `requestAnimationFrame` is a frame handle:
    // the module is an intentional scheduler whose queue state is the stated
    // contract, so none of its functions are flagged.
    #[test]
    fn no_fp_on_animation_frame_scheduler() {
        let src = r#"
const IDLE = 0;
let _animationQueue: Array<() => void> = [];
let _nextAnimationFrame: number = IDLE;

function _runAnimationPhases(): void {
    _nextAnimationFrame = IDLE;
    const queue = _animationQueue;
    _animationQueue = [];
    for (let i = 0; i < queue.length; i++) {
        queue[i]();
    }
}

export function queueAnimation(cb: () => void): void {
    _animationQueue.push(cb);
    if (_nextAnimationFrame === IDLE) {
        _nextAnimationFrame = requestAnimationFrame(_runAnimationPhases);
    }
}

export function hasPendingAnimations(): boolean {
    return _nextAnimationFrame !== IDLE;
}
"#;
        assert!(
            run(src).is_empty(),
            "rAF scheduler functions must not be flagged"
        );
    }

    #[test]
    fn no_fp_on_set_timeout_scheduler() {
        // A `setTimeout`-handle scheduler is the same coordinator pattern.
        let src = r#"
let _queue: Array<() => void> = [];
let _timer: ReturnType<typeof setTimeout> | null = null;

function _flush(): void {
    _timer = null;
    const q = _queue;
    _queue = [];
    q.forEach((cb) => cb());
}

export function schedule(cb: () => void): void {
    _queue.push(cb);
    if (_timer === null) {
        _timer = setTimeout(_flush, 0);
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_mutable_state_without_scheduler_handle() {
        // A plain mutable counter with no scheduling primitive is genuine
        // accidental shared state — the scheduler exception must not apply.
        let src = r#"
let counter = 0;
export function increment() {
    counter += 1;
    return counter;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
        assert!(d[0].message.contains("counter"));
    }

    #[test]
    fn still_flags_when_set_timeout_return_is_discarded() {
        // Calling `setTimeout` without storing its handle in a module-level
        // binding is not the scheduler-coordinator signature; an ordinary
        // function that also mutates shared state stays flagged.
        let src = r#"
let counter = 0;
export function bump() {
    counter += 1;
    setTimeout(() => {}, 0);
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bump"));
    }

    #[test]
    fn no_fp_on_declarator_initialised_scheduler_handle() {
        // The handle can be stored at the declarator, not just reassigned:
        // `let timer = setInterval(...)` at module scope is the same signature.
        let src = r#"
let _ticks = 0;
let _timer = setInterval(() => { _ticks += 1; }, 1000);

export function getTicks(): number {
    return _ticks;
}
"#;
        assert!(run(src).is_empty());
    }

    // Regression for #4723 — write-once lazy-init memoization via `??=`. The
    // module-level `let` starts unset and is seeded exactly once on first call
    // through nullish-assign, so the function returns the same value every time
    // and is idempotent. It must not be flagged.
    #[test]
    fn no_fp_on_nullish_lazy_init_inline() {
        let src = r#"
let availableParallelism: number | undefined;
function getAvailableParallelism() {
    return (availableParallelism ??= Math.max(1, os.availableParallelism()));
}
"#;
        assert!(
            run(src).is_empty(),
            "write-once `??=` lazy-init must not be flagged"
        );
    }

    #[test]
    fn no_fp_on_nullish_lazy_init_two_statements() {
        // The two-statement form: `cache ??= compute(); return cache;`.
        let src = r#"
let cachedEnv: string | undefined;
function readEnvFileCached() {
    cachedEnv ??= readEnvFile();
    return cachedEnv;
}
"#;
        assert!(run(src).is_empty());
    }

    // Regression for #6646 — the `if (cache === undefined) { cache = ...; }`
    // lazy-init idiom is structurally equivalent to `cache ??= ...`: the binding
    // starts unset and is seeded exactly once on first call, behind a guard that
    // tests it for being unset. The reader is idempotent and must not be flagged.
    #[test]
    fn no_fp_on_if_undefined_guarded_lazy_init() {
        let src = r#"
let isDockerCached: boolean;
export function isDocker() {
    if (isDockerCached === undefined) {
        isDockerCached = _hasDockerEnvironment() || _hasDockerCGroup();
    }
    return isDockerCached;
}
"#;
        assert!(
            run(src).is_empty(),
            "if-undefined guarded lazy-init must not be flagged"
        );
    }

    #[test]
    fn no_fp_on_if_eqeq_null_guarded_lazy_init() {
        let src = r#"
let cache: number;
function read() {
    if (cache == null) {
        cache = compute();
    }
    return cache;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_typeof_undefined_guarded_lazy_init() {
        let src = r#"
let cache: number;
function read() {
    if (typeof cache === 'undefined') {
        cache = compute();
    }
    return cache;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_if_undefined_guarded_lazy_init_bare_consequent() {
        // The guard's consequent is a bare statement, not a block.
        let src = r#"
let cache: number;
function read() {
    if (cache === undefined) cache = compute();
    return cache;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_unconditional_plain_assign_to_top_level_state() {
        // A plain `=` write with no if-undefined guard mutates on every call, so
        // the binding is not a write-once cache and the reader stays flagged.
        let src = r#"
let cache: number;
function read() {
    cache = compute();
    return cache;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("read"));
    }

    #[test]
    fn flags_when_guard_tests_a_different_binding() {
        // The guard checks `other`, but the write targets `cache`; the write is
        // not gated on `cache` being unset, so `cache`'s reader stays flagged.
        let src = r#"
let cache: number;
let other: number;
function read() {
    if (other === undefined) {
        cache = compute();
    }
    return cache;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("read"));
        assert!(d[0].message.contains("cache"));
    }

    #[test]
    fn flags_when_guard_uses_inverse_test() {
        // `if (cache !== undefined) { cache = ...; }` writes only when the value
        // is already set — the opposite of lazy-init — so it stays flagged.
        let src = r#"
let cache: number;
function read() {
    if (cache !== undefined) {
        cache = compute();
    }
    return cache;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("read"));
    }

    #[test]
    fn flags_when_assignment_is_in_else_branch() {
        // The write sits in the alternate, not the consequent, so the unset
        // guard does not gate it; the reader stays flagged.
        let src = r#"
let cache: number;
function read() {
    if (cache === undefined) {
        log();
    } else {
        cache = compute();
    }
    return cache;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("read"));
    }

    #[test]
    fn flags_let_reassigned_with_plain_assign_alongside_nullish() {
        // A binding written once via `??=` but also reassigned with a plain `=`
        // elsewhere is genuinely mutable, not a write-once cache, so the
        // reader stays flagged.
        let src = r#"
let cache: number | undefined;
function read() {
    cache ??= compute();
    return cache;
}
function reset() {
    cache = 0;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|x| x.message.contains("read")));
        assert!(d.iter().any(|x| x.message.contains("reset")));
    }

    #[test]
    fn flags_let_with_initializer_even_if_only_nullish_written() {
        // A binding that starts with a concrete value is not the unset-then-
        // seed lazy-init shape; mutating it past that initial value is genuine
        // shared state, so the reader stays flagged.
        let src = r#"
let counter = 0;
function bump() {
    counter ??= 1;
    return counter += 1;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bump"));
    }

    #[test]
    fn still_flags_when_scheduler_handle_is_function_local() {
        // A function-local `let t = setTimeout(...)` is not the module's
        // scheduler handle, so it must not exempt unrelated top-level mutable
        // state read by a sibling function.
        let src = r#"
let counter = 0;
export function arm() {
    let t = setTimeout(() => {}, 0);
    return t;
}
export function increment() {
    counter += 1;
    return counter;
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
        assert!(d[0].message.contains("counter"));
    }

    // Regression for #2240 — a test file's module-level `let` is a deliberate
    // `vi.mock` seam that a mock factory closes over, not accidental shared
    // state. A `vi.mock` factory takes no caller arguments, so the seam cannot
    // be passed as a parameter; `skip_in_test_dir` suppresses the rule there.
    #[test]
    fn no_fp_on_vi_mock_seam_in_test_file() {
        let src = r#"
let mockSearch = {};
const navigateMock = vi.fn();

vi.mock("@tanstack/react-router", () => ({
    useSearch: () => mockSearch,
    useNavigate: () => navigateMock,
}));

function navigateSpy(options: unknown): void {
    recordFunctionalNavigate(options, mockSearch, navigateMock);
}
"#;
        assert!(
            run_gated(src, "src/features/organizations/organizations-page.test.tsx").is_empty(),
            "vi.mock seam read in a test file must not be flagged"
        );
    }

    #[test]
    fn still_flags_same_mutable_state_in_production_file() {
        // The identical shape in a production (non-test) file is genuine shared
        // mutable state and must stay flagged: the test-dir skip is the only
        // thing that exempts it.
        let src = r#"
let counter = 0;
export function increment() {
    counter += 1;
    return counter;
}
"#;
        let d = run_gated(src, "src/features/counter/counter.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
        assert!(d[0].message.contains("counter"));
    }

    // Regression for #6351 — the scoped-context-bracket (push/pop) pattern of a
    // reactive library's "current observer" pointer. `evalContext` is saved,
    // swapped, and restored in a `finally`; `endEffect` pops it back from a
    // parameter. The binding is impure by contract, so its functions must not be
    // flagged.
    #[test]
    fn no_fp_on_scoped_context_bracket_full_repro() {
        let src = r#"
let evalContext: Computed | Effect | undefined = undefined;

function untracked<T>(fn: () => T): T {
    const prevContext = evalContext;
    evalContext = undefined;
    try {
        return fn();
    } finally {
        evalContext = prevContext;
    }
}

function cleanupEffect(effect: Effect) {
    const prevContext = evalContext;
    evalContext = undefined;
    try {
        cleanup();
    } finally {
        evalContext = prevContext;
    }
}

function endEffect(this: Effect, prevContext?: Computed | Effect) {
    if (evalContext !== this) throw new Error("Out-of-order effect");
    evalContext = prevContext;
}
"#;
        assert!(
            run(src).is_empty(),
            "scoped-context-bracket functions must not be flagged"
        );
    }

    #[test]
    fn no_fp_on_minimal_save_modify_finally_restore() {
        // Save, modify, restore-in-finally — the minimal bracket shape.
        let src = r#"
let ctx;
function scope<T>(fn: () => T) {
    const prev = ctx;
    ctx = undefined;
    try {
        return fn();
    } finally {
        ctx = prev;
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_shared_mutable_counter_without_bracket() {
        // A genuine shared mutable counter has no save/restore-in-finally, so it
        // stays flagged — not every module `let` is exempt.
        let src = r#"
let count = 0;
function inc() {
    count = count + 1;
}
"#;
        let d = run(src);
        assert!(!d.is_empty());
        assert!(d.iter().any(|x| x.message.contains("inc")));
    }

    #[test]
    fn still_flags_save_modify_restore_without_finally() {
        // Save, modify, and restore but WITHOUT a `finally`: the unbracketed
        // modify is genuine mutable state and stays flagged.
        let src = r#"
let ctx;
function bad() {
    const p = ctx;
    ctx = 1;
    sideEffect();
    ctx = p;
}
"#;
        let d = run(src);
        assert!(!d.is_empty());
        assert!(d.iter().any(|x| x.message.contains("bad")));
    }
}

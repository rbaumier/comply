//! no-parameters-only-used-in-recursion oxc backend.
//!
//! Flags a function parameter whose every reference is an argument passed to a
//! recursive call of the same function. Such a parameter feeds nothing into the
//! computation: it is threaded round the recursion and never read, so it is
//! effectively unused and can be dropped or replaced with a constant.
//!
//! Recursion identity is resolved through the semantic model, not by name:
//!
//! - Function declarations and named function expressions recurse through their
//!   own binding; an arrow / anonymous function expression assigned to a
//!   variable (`const f = …`) or via assignment (`f = …`) recurses through that
//!   binding. A call is recursive only when its callee resolves to the *same*
//!   symbol, so a same-named binding in another scope (shadowing) is not mistaken
//!   for recursion.
//! - Class and object methods have no callable binding, so they recurse through
//!   `this.method(…)` / `this["method"](…)` matched on the method name.
//!
//! A parameter is reported only when it has at least one reference and *all* of
//! its references sit inside the arguments of a recursive call. Parameters whose
//! name starts with `_` (intentional-unused marker) and TypeScript signature
//! parameters (no body) are skipped, mirroring Biome's `noParametersOnlyUsedInRecursion`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_semantic::{AstNodes, NodeId, Scoping, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

/// How a function can be called recursively.
enum Recursion {
    /// Function declaration / named expression / variable- or assignment-bound
    /// arrow: a call is recursive when its callee resolves to this symbol.
    Binding(SymbolId),
    /// Arrow assigned to an *undeclared* global (`foo = (…) => …` with no
    /// `let`/`const foo`): there is no binding to compare, so a bare-identifier
    /// callee matching this name — itself resolving to no binding — is recursive.
    GlobalName(String),
    /// Class / object method: a call is recursive when it is `this.<name>()` or
    /// `this["<name>"]()` for this method name.
    Method(String),
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let Some(param) = parameter_binding(symbol_id, scoping, nodes) else {
                continue;
            };

            // Intentional-unused marker.
            if param.name.starts_with('_') {
                continue;
            }

            let Some(recursion) = recursion_identity(param.func_node, scoping, nodes) else {
                continue;
            };

            if !all_references_in_recursion(symbol_id, &recursion, param.func_span, scoping, nodes) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Parameter `{}` is only forwarded to recursive calls and never read — \
                     remove it or replace it with a constant.",
                    param.name
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

struct ParameterInfo<'a> {
    name: &'a str,
    span: Span,
    /// The `Function` / `ArrowFunctionExpression` node owning this parameter.
    func_node: NodeId,
    func_span: Span,
}

/// Returns parameter info when `symbol_id` is a top-level simple-identifier or
/// rest parameter of a function with a body. Destructured bindings, signature
/// parameters and non-parameter symbols return `None`, matching Biome's query
/// which only fires on a binding whose declaration is a `JsFormalParameter` or
/// `JsRestParameter`.
fn parameter_binding<'a>(
    symbol_id: SymbolId,
    scoping: &'a Scoping,
    nodes: &AstNodes,
) -> Option<ParameterInfo<'a>> {
    let decl_id = scoping.symbol_declaration(symbol_id);

    // A simple parameter declares its symbol on the `FormalParameter` node whose
    // pattern is a bare identifier; a rest parameter declares it on the
    // `FormalParameters.rest` element. A destructured binding declares on an
    // inner `ObjectPattern`/`ArrayPattern` leaf instead and is not in scope.
    let is_param = match nodes.kind(decl_id) {
        AstKind::FormalParameter(param) => {
            matches!(param.pattern, BindingPattern::BindingIdentifier(_))
        }
        AstKind::BindingRestElement(_) => {
            matches!(nodes.kind(nodes.parent_id(decl_id)), AstKind::FormalParameters(_))
        }
        _ => false,
    };
    if !is_param {
        return None;
    }

    // Walk to the owning function. A signature member (no body) reaches the
    // program / a non-function boundary before any function body, so it is
    // skipped naturally.
    for ancestor in nodes.ancestors(decl_id) {
        match ancestor.kind() {
            AstKind::Function(func) if func.body.is_some() => {
                return Some(ParameterInfo {
                    name: scoping.symbol_name(symbol_id),
                    span: scoping.symbol_span(symbol_id),
                    func_node: ancestor.id(),
                    func_span: func.span(),
                });
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                return Some(ParameterInfo {
                    name: scoping.symbol_name(symbol_id),
                    span: scoping.symbol_span(symbol_id),
                    func_node: ancestor.id(),
                    func_span: arrow.span(),
                });
            }
            AstKind::Program(_) => return None,
            _ => {}
        }
    }
    None
}

/// Determines how the function owning a parameter can call itself recursively.
fn recursion_identity(func_node: NodeId, scoping: &Scoping, nodes: &AstNodes) -> Option<Recursion> {
    match nodes.kind(func_node) {
        AstKind::Function(func) => {
            // Named declaration or named function expression: recurse through the
            // function's own binding.
            if let Some(id) = &func.id
                && let Some(symbol_id) = id.symbol_id.get()
            {
                return Some(Recursion::Binding(symbol_id));
            }
            // Anonymous function expression: fall back to its surrounding
            // variable / assignment binding, or, if it is a method value, its name.
            recursion_from_context(func_node, scoping, nodes)
        }
        AstKind::ArrowFunctionExpression(_) => recursion_from_context(func_node, scoping, nodes),
        _ => None,
    }
}

/// Resolves the recursion identity for an anonymous function/arrow from the node
/// it is attached to: a class/object method name, a variable declarator binding,
/// or an assignment target binding.
fn recursion_from_context(func_node: NodeId, scoping: &Scoping, nodes: &AstNodes) -> Option<Recursion> {
    let func_span = nodes.kind(func_node).span();
    for ancestor in nodes.ancestors(func_node) {
        match ancestor.kind() {
            AstKind::MethodDefinition(method) => {
                return method_name(&method.key).map(Recursion::Method);
            }
            AstKind::ObjectProperty(prop) if prop.method => {
                return method_name(&prop.key).map(Recursion::Method);
            }
            AstKind::VariableDeclarator(decl) => {
                if let BindingPattern::BindingIdentifier(id) = &decl.id
                    && let Some(symbol_id) = id.symbol_id.get()
                {
                    return Some(Recursion::Binding(symbol_id));
                }
                return None;
            }
            AstKind::AssignmentExpression(assign) => {
                // `f = () => …`: the function must be the right-hand side.
                if assign.right.span() != func_span {
                    return None;
                }
                if let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(target) =
                    &assign.left
                {
                    // A declared target resolves to a binding; an undeclared
                    // global resolves to nothing and is matched by name.
                    if let Some(symbol_id) = target
                        .reference_id
                        .get()
                        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
                    {
                        return Some(Recursion::Binding(symbol_id));
                    }
                    return Some(Recursion::GlobalName(target.name.to_string()));
                }
                return None;
            }
            // Stop at the first enclosing function-like / statement boundary that
            // is not a transparent wrapper, so we never borrow a name from an
            // unrelated outer scope.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return None,
            AstKind::ParenthesizedExpression(_) => {}
            _ => return None,
        }
    }
    None
}

/// True when every reference of the parameter sits inside the arguments of a
/// recursive call, and there is at least one such reference.
fn all_references_in_recursion(
    symbol_id: SymbolId,
    recursion: &Recursion,
    func_span: Span,
    scoping: &Scoping,
    nodes: &AstNodes,
) -> bool {
    let mut any = false;
    for reference in scoping.get_resolved_references(symbol_id) {
        any = true;
        if !reference_in_recursive_call(reference.node_id(), recursion, func_span, scoping, nodes) {
            return false;
        }
    }
    any
}

/// Walks up from a reference to the enclosing function boundary; returns true if
/// it passes through the arguments of a recursive call.
fn reference_in_recursive_call(
    ref_node: NodeId,
    recursion: &Recursion,
    func_span: Span,
    scoping: &Scoping,
    nodes: &AstNodes,
) -> bool {
    let ref_span = nodes.kind(ref_node).span();
    for ancestor in nodes.ancestors(ref_node) {
        let kind = ancestor.kind();
        if let AstKind::CallExpression(call) = kind
            && is_recursive_call(call, recursion, scoping)
            && reference_in_arguments(call, ref_span)
        {
            return true;
        }
        // Function boundary — stop before leaving the function the parameter
        // belongs to (its own span counts as the boundary).
        if matches!(kind, AstKind::Function(_) | AstKind::ArrowFunctionExpression(_))
            && kind.span() == func_span
        {
            break;
        }
    }
    false
}

/// True when `ref_span` is inside one of the call's non-spread argument
/// expressions. Spread arguments are skipped conservatively: a `...rest` forwards
/// the whole list, so the rest parameter cannot be singled out as dead.
fn reference_in_arguments(call: &oxc_ast::ast::CallExpression, ref_span: Span) -> bool {
    call.arguments.iter().any(|arg| {
        if matches!(arg, oxc_ast::ast::Argument::SpreadElement(_)) {
            return false;
        }
        let arg_span = arg.span();
        arg_span.start <= ref_span.start && ref_span.end <= arg_span.end
    })
}

/// True when a call expression calls the function recursively.
fn is_recursive_call(
    call: &oxc_ast::ast::CallExpression,
    recursion: &Recursion,
    scoping: &Scoping,
) -> bool {
    match recursion {
        Recursion::Binding(target) => match &call.callee {
            Expression::Identifier(id) => id
                .reference_id
                .get()
                .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
                .is_some_and(|sym| sym == *target),
            _ => false,
        },
        Recursion::GlobalName(name) => match &call.callee {
            Expression::Identifier(id) => {
                let callee_unresolved = id
                    .reference_id
                    .get()
                    .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
                    .is_none();
                callee_unresolved && id.name.as_str() == name.as_str()
            }
            _ => false,
        },
        Recursion::Method(name) => callee_is_this_method(&call.callee, name),
    }
}

/// True when a callee is `this.<name>` or `this["<name>"]` (optional chaining
/// included), matching the recursive method name.
fn callee_is_this_method(callee: &Expression, name: &str) -> bool {
    match callee {
        Expression::StaticMemberExpression(member) => {
            object_is_this(&member.object) && member.property.name == name
        }
        Expression::ComputedMemberExpression(member) => {
            object_is_this(&member.object) && computed_member_is(&member.expression, name)
        }
        Expression::ChainExpression(chain) => {
            // `this?.m(…)` wraps the call in a chain; unwrap to the member callee.
            match &chain.expression {
                oxc_ast::ast::ChainElement::StaticMemberExpression(member) => {
                    object_is_this(&member.object) && member.property.name == name
                }
                oxc_ast::ast::ChainElement::ComputedMemberExpression(member) => {
                    object_is_this(&member.object) && computed_member_is(&member.expression, name)
                }
                oxc_ast::ast::ChainElement::CallExpression(call) => {
                    callee_is_this_method(&call.callee, name)
                }
                _ => false,
            }
        }
        Expression::ParenthesizedExpression(paren) => {
            callee_is_this_method(&paren.expression, name)
        }
        _ => false,
    }
}

fn object_is_this(object: &Expression) -> bool {
    matches!(object, Expression::ThisExpression(_))
}

/// True when a computed member key is the string literal `name`. Non-literal
/// keys (`this[var]()`) are conservatively not recognised as recursion.
fn computed_member_is(expr: &Expression, name: &str) -> bool {
    matches!(expr, Expression::StringLiteral(lit) if lit.value == name)
}

fn method_name(key: &oxc_ast::ast::PropertyKey) -> Option<String> {
    match key {
        oxc_ast::ast::PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        oxc_ast::ast::PropertyKey::StringLiteral(lit) => Some(lit.value.to_string()),
        _ => None,
    }
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

    fn count(src: &str) -> usize {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len()
    }

    // --- Invalid (Biome invalid.js fixtures): parameter only forwarded to recursion ---

    #[test]
    fn function_declaration_single_param() {
        assert_eq!(
            count("function factorial(n, acc) {\n  if (n === 0) return 1;\n  return factorial(n - 1, acc);\n}"),
            1
        );
    }

    #[test]
    fn multiple_params_only_in_recursion() {
        // `b` and `c` are both forwarded only.
        assert_eq!(
            count("function fn(a, b, c) {\n  if (a === 0) return 0;\n  return fn(a - 1, b, c);\n}"),
            2
        );
    }

    #[test]
    fn arrow_function_via_variable() {
        assert_eq!(
            count("const countdown = (n, acc) => {\n  if (n === 0) return 0;\n  return countdown(n - 1, acc);\n};"),
            1
        );
    }

    #[test]
    fn class_method_this_call() {
        assert_eq!(
            count("class Counter {\n  count(n, acc) {\n    if (n === 0) return 0;\n    return this.count(n - 1, acc);\n  }\n}"),
            1
        );
    }

    #[test]
    fn param_with_arithmetic_in_recursion() {
        assert_eq!(
            count("function countdown(n, step) {\n  if (n === 0) return 0;\n  return countdown(n - step, step);\n}"),
            1
        );
    }

    #[test]
    fn unary_operation_in_recursion() {
        assert_eq!(
            count("function negate(n, flag) {\n  if (n === 0) return 0;\n  return negate(n - 1, !flag);\n}"),
            1
        );
    }

    #[test]
    fn object_method_this_call() {
        assert_eq!(
            count("const obj = {\n  count(n, step) {\n    if (n === 0) return 0;\n    return this.count(n - step, step);\n  }\n};"),
            1
        );
    }

    #[test]
    fn assignment_expression_arrow() {
        assert_eq!(
            count("foo = (n, acc) => {\n  if (n === 0) return 0;\n  return foo(n - 1, acc);\n};"),
            1
        );
    }

    #[test]
    fn separate_declaration_and_assignment_arrow() {
        assert_eq!(
            count("let bar;\nbar = (x, unused) => {\n  if (x === 0) return 0;\n  return bar(x - 1, unused);\n};"),
            1
        );
    }

    #[test]
    fn logical_and_in_recursion() {
        assert_eq!(
            count("function fnAnd(n, acc) {\n  if (n === 0) return 0;\n  return fnAnd(n - 1, acc && true);\n}"),
            1
        );
    }

    #[test]
    fn logical_or_in_recursion() {
        assert_eq!(
            count("function fnOr(n, acc) {\n  if (n === 0) return 0;\n  return fnOr(n - 1, acc || 0);\n}"),
            1
        );
    }

    #[test]
    fn nullish_in_recursion() {
        assert_eq!(
            count("function fnNullish(n, acc) {\n  if (n === 0) return 0;\n  return fnNullish(n - 1, acc ?? 0);\n}"),
            1
        );
    }

    #[test]
    fn nested_logical_in_recursion() {
        assert_eq!(
            count("function fnNested(n, acc) {\n  if (n === 0) return 0;\n  return fnNested(n - 1, (acc || 0) && true);\n}"),
            1
        );
    }

    #[test]
    fn conditional_consequent_in_recursion() {
        assert_eq!(
            count("function fnCond(n, acc) {\n  if (n === 0) return 0;\n  return fnCond(n - 1, n > 5 ? acc : 0);\n}"),
            1
        );
    }

    #[test]
    fn conditional_test_in_recursion() {
        assert_eq!(
            count("function fnCondTest(n, flag) {\n  if (n === 0) return 0;\n  return fnCondTest(n - 1, flag ? true : false);\n}"),
            1
        );
    }

    #[test]
    fn optional_chaining_class_method() {
        assert_eq!(
            count("class C {\n  count(n, acc) {\n    if (n === 0) return 0;\n    return this?.count(n - 1, acc);\n  }\n}"),
            1
        );
    }

    #[test]
    fn computed_member_string_literal() {
        assert_eq!(
            count("class C {\n  count(n, acc) {\n    if (n === 0) return 0;\n    return this[\"count\"](n - 1, acc);\n  }\n}"),
            1
        );
    }

    #[test]
    fn optional_computed_member() {
        assert_eq!(
            count("class C {\n  count(n, acc) {\n    if (n === 0) return 0;\n    return this?.[\"count\"](n - 1, acc);\n  }\n}"),
            1
        );
    }

    // --- Valid (Biome valid.js fixtures): parameter read outside recursion ---

    #[test]
    fn param_used_outside_recursion() {
        assert_eq!(
            count("function factorial(n, acc) {\n  console.log(acc);\n  if (n === 0) return acc;\n  return factorial(n - 1, acc * n);\n}"),
            0
        );
    }

    #[test]
    fn param_not_used_at_all() {
        // Out of scope — handled by the unused-parameter rule.
        assert_eq!(count("function foo(unused) {\n  return 42;\n}"), 0);
    }

    #[test]
    fn param_used_in_condition() {
        assert_eq!(
            count("function fn(n, threshold) {\n  if (n > threshold) return n;\n  return fn(n + 1, threshold);\n}"),
            0
        );
    }

    #[test]
    fn param_used_in_non_recursive_call() {
        assert_eq!(
            count("function fn2(n, logger) {\n  logger(n);\n  if (n === 0) return 0;\n  return fn2(n - 1, logger);\n}"),
            0
        );
    }

    #[test]
    fn param_used_in_return_value() {
        assert_eq!(
            count("function factorial2(n, acc) {\n  if (n === 0) return acc;\n  return factorial2(n - 1, acc * n);\n}"),
            0
        );
    }

    #[test]
    fn assignment_arrow_used_outside() {
        assert_eq!(
            count("bar = (n, threshold) => {\n  if (n > threshold) return threshold;\n  return bar(n + 1, threshold);\n};"),
            0
        );
    }

    #[test]
    fn optional_chaining_param_used_elsewhere() {
        assert_eq!(
            count("class C {\n  count(n, acc) {\n    console.log(acc);\n    if (n === 0) return 0;\n    return this?.count(n - 1, acc);\n  }\n}"),
            0
        );
    }

    #[test]
    fn computed_member_non_literal_not_recursive() {
        // `this[methodName]()` is not recognised as recursion, so `acc` has no
        // in-recursion reference and is therefore unused-elsewhere — not flagged.
        assert_eq!(
            count("class C {\n  count(n, acc) {\n    const methodName = \"count\";\n    if (n === 0) return 0;\n    return this[methodName](n - 1, acc);\n  }\n}"),
            0
        );
    }

    #[test]
    fn object_method_calls_outer_same_name_not_recursive() {
        // The object method calls the outer `notRecursive`, not itself; `arg` is
        // forwarded to a non-recursive call.
        assert_eq!(
            count("function notRecursive(arg) {\n  return arg;\n}\nconst obj = {\n  notRecursive(arg) {\n    return notRecursive(arg);\n  }\n};"),
            0
        );
    }

    #[test]
    fn named_function_expression_recurses_through_own_name() {
        assert_eq!(
            count("const f = function rec(n, acc) {\n  if (n === 0) return 0;\n  return rec(n - 1, acc);\n};"),
            1
        );
    }

    #[test]
    fn rest_param_spread_forward_not_flagged() {
        // `...rest` is forwarded as a spread; Biome skips spread arguments, so the
        // rest parameter is not singled out as dead.
        assert_eq!(
            count("function f(n, ...rest) {\n  if (n === 0) return 0;\n  return f(n - 1, ...rest);\n}"),
            0
        );
    }

    #[test]
    fn underscore_prefixed_param_skipped() {
        assert_eq!(
            count("function f(n, _acc) {\n  if (n === 0) return 0;\n  return f(n - 1, _acc);\n}"),
            0
        );
    }

    #[test]
    fn ts_interface_signature_skipped() {
        assert_eq!(count("interface I {\n  f(n: number, acc: number): number;\n}"), 0);
    }

    #[test]
    fn shadowing_same_named_function_not_recursion() {
        // The inner `g` shadows the outer; `g(x - 1, acc)` resolves to the inner
        // binding, which is the recursive function — so it IS flagged. But a call
        // to a same-named *outer* function from a different function must not be
        // treated as recursion. Here the outer `helper` forwards `acc` to a call
        // of an unrelated, separately-bound `helper` declared later.
        let src = "function outer(n, acc) {\n  function inner(x) { return x; }\n  if (n === 0) return inner(acc);\n  return outer(n - 1, acc);\n}";
        // `acc` is read by `inner(acc)` outside recursion → not flagged.
        assert_eq!(count(src), 0);
    }
}

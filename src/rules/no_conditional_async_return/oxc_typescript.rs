//! OxcCheck backend for no-conditional-async-return.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BinaryExpression, BinaryOperator, BindingPattern, Expression, FormalParameters, IfStatement,
    LogicalOperator, Statement, TSType, TSTypeAnnotation, TSTypeName, UnaryOperator,
};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnKind {
    Sync,
    Promise,
    /// The rejection/error channel — `Promise.reject(...)`, semantically a
    /// `throw`. It never resolves to a usable value, so it participates in
    /// neither the sync nor the promise side of the mix, exactly like a `throw`
    /// statement that produces no return value at all.
    Error,
    /// A bare-identifier return whose binding does not resolve to a determinable
    /// sync or promise initializer (an opaque parameter, import, or unresolvable
    /// value). Like [`ReturnKind::Error`] it is evidence of neither side of the
    /// mix, so it never contributes a spurious sync branch.
    Unknown,
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
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::Function(func) => {
                    if func.r#async {
                        continue;
                    }
                    let Some(body) = &func.body else {
                        continue;
                    };
                    if annotation_permits_promise_branches(func.return_type.as_deref(), semantic) {
                        continue;
                    }
                    let kinds = collect_return_kinds(&body.statements, ctx.source, semantic);
                    if kinds.contains(&ReturnKind::Sync)
                        && kinds.contains(&ReturnKind::Promise)
                        && !is_callback_promise_dual_mode(
                            &func.params,
                            &body.statements,
                            ctx.source,
                            semantic,
                        )
                        && !is_promise_passthrough_dual_mode(&body.statements)
                    {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, func.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Function mixes sync and promise-returning branches — unify to `Promise<T>` (async) or plain `T` everywhere.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    if arrow.r#async {
                        continue;
                    }
                    if arrow.expression {
                        continue;
                    }
                    if annotation_permits_promise_branches(arrow.return_type.as_deref(), semantic) {
                        continue;
                    }
                    let kinds = collect_return_kinds(&arrow.body.statements, ctx.source, semantic);
                    if kinds.contains(&ReturnKind::Sync)
                        && kinds.contains(&ReturnKind::Promise)
                        && !is_callback_promise_dual_mode(
                            &arrow.params,
                            &arrow.body.statements,
                            ctx.source,
                            semantic,
                        )
                        && !is_promise_passthrough_dual_mode(&arrow.body.statements)
                    {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Function mixes sync and promise-returning branches — unify to `Promise<T>` (async) or plain `T` everywhere.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

/// Is this type a `Promise<...>` reference?
fn is_promise_type(ty: &TSType) -> bool {
    if let TSType::TSTypeReference(tref) = ty
        && let TSTypeName::IdentifierReference(id) = &tref.type_name
    {
        return id.name.as_str() == "Promise";
    }
    false
}

/// Does the explicit return-type annotation document an all-promise or
/// mixed-promise contract? A plain `Promise<T>` guarantees every branch is a
/// Promise (TypeScript enforces assignability), and a `Promise<T>`-bearing
/// union (e.g. `void | Promise<void>`, `T | Promise<T>`) deliberately documents
/// the mixed-return contract. `ReturnType<F>` where `F` is an enclosing-scope
/// type parameter (e.g. `ReturnType<CallFunction>`) is equally abstract: the
/// return type is deferred to whatever the generic function returns, so a body
/// that is sync in one branch and a Promise in another mirrors the generic
/// contract exactly, just like an explicit `T | Promise<T>` union. In all these
/// cases the conditional return is intentional, so trust the annotation rather
/// than the syntactic classifier.
fn annotation_permits_promise_branches(
    return_type: Option<&TSTypeAnnotation>,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(rt) = return_type else { return false };
    let mut ty = &rt.type_annotation;
    while let TSType::TSParenthesizedType(paren) = ty {
        ty = &paren.type_annotation;
    }
    if is_promise_type(ty) {
        return true;
    }
    if is_returntype_of_type_parameter(ty, semantic) {
        return true;
    }
    let TSType::TSUnionType(union) = ty else {
        return false;
    };
    union.types.iter().any(is_promise_type) && union.types.iter().any(|t| !is_promise_type(t))
}

/// Is this annotation the built-in `ReturnType<F>` utility whose type argument
/// references an enclosing-scope type parameter? The type parameter is resolved
/// via the semantic model (not by name), so `ReturnType<typeof concreteFn>` or
/// `ReturnType<SomeConcreteType>` — whose argument is not a type parameter — is
/// not matched and a mixed-return body there is still flagged.
fn is_returntype_of_type_parameter(ty: &TSType, semantic: &oxc_semantic::Semantic) -> bool {
    let TSType::TSTypeReference(tref) = ty else {
        return false;
    };
    let TSTypeName::IdentifierReference(id) = &tref.type_name else {
        return false;
    };
    if id.name.as_str() != "ReturnType" {
        return false;
    }
    tref.type_arguments.as_ref().is_some_and(|args| {
        args.params.iter().any(|p| {
            crate::oxc_helpers::type_references_enclosing_type_parameter(p, semantic)
        })
    })
}

/// Classify a return-value expression as promise-returning or sync.
fn classify_value(
    expr: &Expression,
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> ReturnKind {
    // `a ?? Promise.resolve()` / `a || Promise.resolve()`: a single
    // Promise-typed operand makes the whole expression resolve to a Promise.
    if let Expression::LogicalExpression(logical) = expr
        && matches!(logical.operator, LogicalOperator::Coalesce | LogicalOperator::Or)
        && (classify_value(&logical.left, source, semantic) == ReturnKind::Promise
            || classify_value(&logical.right, source, semantic) == ReturnKind::Promise)
    {
        return ReturnKind::Promise;
    }

    // `new Promise(...)` constructs a Promise regardless of its executor. Matched
    // by constructor name, like the `Promise.<combinator>` check below.
    if let Expression::NewExpression(new_expr) = expr
        && let Expression::Identifier(id) = &new_expr.callee
        && id.name.as_str() == "Promise"
    {
        return ReturnKind::Promise;
    }

    // A bare identifier is not assumed sync: resolve its binding and classify by
    // the initializer, so `const p = load(); return p;` (where `load` returns a
    // `Promise`) is a promise branch, not a spurious sync one.
    if let Expression::Identifier(id) = expr {
        return match crate::oxc_helpers::classify_identifier_binding_return(id, semantic) {
            crate::oxc_helpers::BindingReturnKind::Async => ReturnKind::Promise,
            crate::oxc_helpers::BindingReturnKind::Sync => ReturnKind::Sync,
            crate::oxc_helpers::BindingReturnKind::Unknown => ReturnKind::Unknown,
        };
    }

    let Expression::CallExpression(call) = expr else {
        return ReturnKind::Sync;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return ReturnKind::Sync;
    };
    let method = member.property.name.as_str();

    // `.then(...)` / `.catch(...)` / `.finally(...)` on any receiver
    if method == "then" || method == "catch" || method == "finally" {
        return ReturnKind::Promise;
    }

    // `Promise.<combinator>(...)`
    if let Expression::Identifier(obj) = &member.object
        && obj.name.as_str() == "Promise"
    {
        // `Promise.reject(...)` is the rejection/error channel — semantically a
        // `throw`, never a resolvable value — so it does not count as a
        // promise-value branch (and must not be miscounted as sync either).
        if method == "reject" {
            return ReturnKind::Error;
        }
        if matches!(method, "resolve" | "all" | "allSettled" | "race" | "any") {
            return ReturnKind::Promise;
        }
    }

    ReturnKind::Sync
}

/// Walk statements collecting return kinds. Skip nested function bodies.
fn collect_return_kinds(
    stmts: &[Statement],
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> Vec<ReturnKind> {
    let mut out = Vec::new();
    for stmt in stmts {
        collect_from_stmt(stmt, source, semantic, None, &mut out);
    }
    out
}

/// Walk a statement collecting return kinds, skipping nested function bodies.
/// When `exclude` is set, returns whose span falls inside it are ignored — used
/// to look for a Promise return *outside* a dual-mode callback branch.
fn collect_from_stmt(
    stmt: &Statement,
    source: &str,
    semantic: &oxc_semantic::Semantic,
    exclude: Option<Span>,
    out: &mut Vec<ReturnKind>,
) {
    match stmt {
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                let skipped = exclude.is_some_and(|ex| span_contains(ex, ret.span));
                if !skipped {
                    out.push(classify_value(arg, source, semantic));
                }
            }
        }
        // Don't descend into nested functions
        Statement::FunctionDeclaration(_) => {}
        Statement::BlockStatement(block) => {
            for s in &block.body {
                collect_from_stmt(s, source, semantic, exclude, out);
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_from_stmt(&if_stmt.consequent, source, semantic, exclude, out);
            if let Some(alt) = &if_stmt.alternate {
                collect_from_stmt(alt, source, semantic, exclude, out);
            }
        }
        Statement::SwitchStatement(switch) => {
            for case in &switch.cases {
                for s in &case.consequent {
                    collect_from_stmt(s, source, semantic, exclude, out);
                }
            }
        }
        Statement::TryStatement(try_stmt) => {
            for s in &try_stmt.block.body {
                collect_from_stmt(s, source, semantic, exclude, out);
            }
            if let Some(handler) = &try_stmt.handler {
                for s in &handler.body.body {
                    collect_from_stmt(s, source, semantic, exclude, out);
                }
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                for s in &finalizer.body {
                    collect_from_stmt(s, source, semantic, exclude, out);
                }
            }
        }
        Statement::ForStatement(for_stmt) => {
            collect_from_stmt(&for_stmt.body, source, semantic, exclude, out);
        }
        Statement::ForInStatement(for_in) => {
            collect_from_stmt(&for_in.body, source, semantic, exclude, out);
        }
        Statement::ForOfStatement(for_of) => {
            collect_from_stmt(&for_of.body, source, semantic, exclude, out);
        }
        Statement::WhileStatement(while_stmt) => {
            collect_from_stmt(&while_stmt.body, source, semantic, exclude, out);
        }
        Statement::DoWhileStatement(do_while) => {
            collect_from_stmt(&do_while.body, source, semantic, exclude, out);
        }
        Statement::LabeledStatement(labeled) => {
            collect_from_stmt(&labeled.body, source, semantic, exclude, out);
        }
        // ExpressionStatement containing arrow/function — skip (nested fn)
        Statement::ExpressionStatement(es) => {
            if matches!(
                es.expression,
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            ) {
                // nested function — don't descend
            }
        }
        _ => {}
    }
}

/// True when the function is a Node.js callback/promise dual-mode API: the mixed
/// Sync/Promise returns are gated on whether a callback parameter was supplied.
/// When the callback is present its result is delivered by invoking it (and the
/// call returns void/sync); when it is omitted a Promise is returned. This is
/// recognised structurally so any `fn(opts, cb)` dual-mode shape is spared,
/// without keying on a parameter name. All three must hold for some `if`:
///   1. the test is a callback presence/absence check on a parameter `P`,
///   2. the branch where `P` is present invokes `P` (`P(...)`/`P.call`/`P.apply`),
///   3. a Promise is returned outside that present branch.
fn is_callback_promise_dual_mode(
    params: &FormalParameters,
    body: &[Statement],
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let param_names = collect_simple_param_names(params);
    if param_names.is_empty() {
        return false;
    }
    any_dual_mode_guard(body, body, &param_names, source, semantic)
}

/// Names of the function's plain identifier parameters (including a defaulted
/// `cb = undefined`). Destructured parameters cannot be a callback binding, so
/// they are skipped.
fn collect_simple_param_names<'a>(params: &'a FormalParameters<'a>) -> Vec<&'a str> {
    params
        .items
        .iter()
        .filter_map(|item| simple_binding_name(&item.pattern))
        .collect()
}

fn simple_binding_name<'a>(pattern: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        BindingPattern::AssignmentPattern(assign) => simple_binding_name(&assign.left),
        _ => None,
    }
}

/// `root` is the whole function body (for the "Promise returned outside"
/// condition); `stmts` is the slice currently scanned for a guard. Descends
/// through control flow but not into nested function bodies.
fn any_dual_mode_guard(
    root: &[Statement],
    stmts: &[Statement],
    params: &[&str],
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_has_dual_mode_guard(root, stmt, params, source, semantic))
}

fn stmt_has_dual_mode_guard(
    root: &[Statement],
    stmt: &Statement,
    params: &[&str],
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    match stmt {
        Statement::IfStatement(if_stmt) => {
            if_is_dual_mode_guard(if_stmt, root, params, source, semantic)
                || stmt_has_dual_mode_guard(root, &if_stmt.consequent, params, source, semantic)
                || if_stmt.alternate.as_ref().is_some_and(|alt| {
                    stmt_has_dual_mode_guard(root, alt, params, source, semantic)
                })
        }
        Statement::BlockStatement(block) => {
            any_dual_mode_guard(root, &block.body, params, source, semantic)
        }
        Statement::TryStatement(try_stmt) => {
            any_dual_mode_guard(root, &try_stmt.block.body, params, source, semantic)
                || try_stmt.handler.as_ref().is_some_and(|h| {
                    any_dual_mode_guard(root, &h.body.body, params, source, semantic)
                })
                || try_stmt.finalizer.as_ref().is_some_and(|f| {
                    any_dual_mode_guard(root, &f.body, params, source, semantic)
                })
        }
        Statement::ForStatement(f) => {
            stmt_has_dual_mode_guard(root, &f.body, params, source, semantic)
        }
        Statement::ForInStatement(f) => {
            stmt_has_dual_mode_guard(root, &f.body, params, source, semantic)
        }
        Statement::ForOfStatement(f) => {
            stmt_has_dual_mode_guard(root, &f.body, params, source, semantic)
        }
        Statement::WhileStatement(w) => {
            stmt_has_dual_mode_guard(root, &w.body, params, source, semantic)
        }
        Statement::DoWhileStatement(d) => {
            stmt_has_dual_mode_guard(root, &d.body, params, source, semantic)
        }
        Statement::SwitchStatement(switch) => switch.cases.iter().any(|case| {
            case.consequent
                .iter()
                .any(|s| stmt_has_dual_mode_guard(root, s, params, source, semantic))
        }),
        Statement::LabeledStatement(l) => {
            stmt_has_dual_mode_guard(root, &l.body, params, source, semantic)
        }
        _ => false,
    }
}

/// Does this `if` implement the dual-mode guard for one of `params`?
fn if_is_dual_mode_guard(
    if_stmt: &IfStatement,
    root: &[Statement],
    params: &[&str],
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some((param, present_in_consequent)) = classify_callback_guard(&if_stmt.test, params)
    else {
        return false;
    };
    let present: Option<&Statement> = if present_in_consequent {
        Some(&if_stmt.consequent)
    } else {
        if_stmt.alternate.as_ref()
    };
    let Some(present) = present else {
        return false;
    };
    branch_invokes_param(present, param)
        && returns_promise_outside(root, source, semantic, present.span())
}

/// Classify an `if` test as a callback presence/absence check on one of `params`.
/// Returns the matched parameter name and whether the parameter is *present* in
/// the consequent branch (`true`) or the alternate branch (`false`). Recognised
/// forms: `cb`, `cb !== undefined`/`cb != null`, `cb === undefined`/`cb == null`,
/// `typeof cb === 'function'`, `typeof cb !== 'function'`.
fn classify_callback_guard<'a>(test: &Expression, params: &[&'a str]) -> Option<(&'a str, bool)> {
    match test {
        Expression::Identifier(id) => param_name(id.name.as_str(), params).map(|p| (p, true)),
        Expression::BinaryExpression(bin) => classify_binary_guard(bin, params),
        _ => None,
    }
}

fn classify_binary_guard<'a>(
    bin: &BinaryExpression,
    params: &[&'a str],
) -> Option<(&'a str, bool)> {
    let is_eq = matches!(
        bin.operator,
        BinaryOperator::Equality | BinaryOperator::StrictEquality
    );
    let is_neq = matches!(
        bin.operator,
        BinaryOperator::Inequality | BinaryOperator::StrictInequality
    );
    if !is_eq && !is_neq {
        return None;
    }

    // `typeof cb === 'function'` → present in consequent; `!==` → in alternate.
    let typeof_param = typeof_param_operand(&bin.left, params)
        .filter(|_| is_function_string(&bin.right))
        .or_else(|| {
            typeof_param_operand(&bin.right, params).filter(|_| is_function_string(&bin.left))
        });
    if let Some(param) = typeof_param {
        return Some((param, is_eq));
    }

    // `cb === undefined`/`== null` → present in alternate; `!==`/`!=` → consequent.
    let param = param_operand(&bin.left, params)
        .filter(|_| is_nullish_literal(&bin.right))
        .or_else(|| param_operand(&bin.right, params).filter(|_| is_nullish_literal(&bin.left)));
    param.map(|p| (p, is_neq))
}

fn param_name<'a>(name: &str, params: &[&'a str]) -> Option<&'a str> {
    params.iter().copied().find(|p| *p == name)
}

fn param_operand<'a>(expr: &Expression, params: &[&'a str]) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => param_name(id.name.as_str(), params),
        _ => None,
    }
}

fn typeof_param_operand<'a>(expr: &Expression, params: &[&'a str]) -> Option<&'a str> {
    let Expression::UnaryExpression(unary) = expr else {
        return None;
    };
    if unary.operator != UnaryOperator::Typeof {
        return None;
    }
    param_operand(&unary.argument, params)
}

fn is_function_string(expr: &Expression) -> bool {
    matches!(expr, Expression::StringLiteral(lit) if lit.value == "function")
}

fn is_nullish_literal(expr: &Expression) -> bool {
    match expr {
        Expression::NullLiteral(_) => true,
        Expression::Identifier(id) => id.name.as_str() == "undefined",
        _ => false,
    }
}

/// Is `param` invoked as a function anywhere in this branch? Descends through
/// control flow but not into nested function bodies (a callback merely *passed*
/// to an inner closure does not count as invoked here).
fn branch_invokes_param(stmt: &Statement, param: &str) -> bool {
    match stmt {
        Statement::ExpressionStatement(es) => expr_invokes_param(&es.expression, param),
        Statement::ReturnStatement(ret) => ret
            .argument
            .as_ref()
            .is_some_and(|arg| expr_invokes_param(arg, param)),
        Statement::VariableDeclaration(decl) => decl
            .declarations
            .iter()
            .any(|d| d.init.as_ref().is_some_and(|e| expr_invokes_param(e, param))),
        Statement::BlockStatement(block) => {
            block.body.iter().any(|s| branch_invokes_param(s, param))
        }
        Statement::IfStatement(if_stmt) => {
            branch_invokes_param(&if_stmt.consequent, param)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|alt| branch_invokes_param(alt, param))
        }
        Statement::TryStatement(try_stmt) => {
            try_stmt
                .block
                .body
                .iter()
                .any(|s| branch_invokes_param(s, param))
                || try_stmt
                    .handler
                    .as_ref()
                    .is_some_and(|h| h.body.body.iter().any(|s| branch_invokes_param(s, param)))
                || try_stmt
                    .finalizer
                    .as_ref()
                    .is_some_and(|f| f.body.iter().any(|s| branch_invokes_param(s, param)))
        }
        Statement::ForStatement(f) => branch_invokes_param(&f.body, param),
        Statement::ForInStatement(f) => branch_invokes_param(&f.body, param),
        Statement::ForOfStatement(f) => branch_invokes_param(&f.body, param),
        Statement::WhileStatement(w) => branch_invokes_param(&w.body, param),
        Statement::DoWhileStatement(d) => branch_invokes_param(&d.body, param),
        Statement::SwitchStatement(switch) => switch
            .cases
            .iter()
            .any(|c| c.consequent.iter().any(|s| branch_invokes_param(s, param))),
        Statement::LabeledStatement(l) => branch_invokes_param(&l.body, param),
        _ => false,
    }
}

fn expr_invokes_param(expr: &Expression, param: &str) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            callee_is_param(&call.callee, param)
                || call.arguments.iter().any(|arg| {
                    arg.as_expression()
                        .is_some_and(|e| expr_invokes_param(e, param))
                })
        }
        Expression::AwaitExpression(a) => expr_invokes_param(&a.argument, param),
        Expression::ParenthesizedExpression(p) => expr_invokes_param(&p.expression, param),
        Expression::SequenceExpression(seq) => {
            seq.expressions.iter().any(|e| expr_invokes_param(e, param))
        }
        _ => false,
    }
}

fn callee_is_param(callee: &Expression, param: &str) -> bool {
    match callee {
        Expression::Identifier(id) => id.name.as_str() == param,
        Expression::StaticMemberExpression(member) => {
            matches!(member.property.name.as_str(), "call" | "apply")
                && matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == param)
        }
        _ => false,
    }
}

/// Is a Promise returned anywhere in `stmts` outside the `exclude` span (the
/// dual-mode present/callback branch)?
fn returns_promise_outside(
    stmts: &[Statement],
    source: &str,
    semantic: &oxc_semantic::Semantic,
    exclude: Span,
) -> bool {
    let mut kinds = Vec::new();
    for stmt in stmts {
        collect_from_stmt(stmt, source, semantic, Some(exclude), &mut kinds);
    }
    kinds.contains(&ReturnKind::Promise)
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// True when the function is an isomorphic sync/async passthrough discriminated
/// by `<X> instanceof Promise`: the promise branch returns a
/// `.then`/`.catch`/`.finally` chain on `<X>` and the sync branch returns the
/// raw `<X>`. Both arms forward the same binding `<X>`, so the mixed
/// Sync/Promise returns mirror whatever the callback that produced `<X>` returned
/// rather than being an accidental inconsistency. Recognised structurally on the
/// `instanceof Promise` test and the shared returned binding, never by name.
fn is_promise_passthrough_dual_mode(body: &[Statement]) -> bool {
    passthrough_guard_in_stmts(body)
}

fn passthrough_guard_in_stmts(stmts: &[Statement]) -> bool {
    stmts.iter().any(stmt_has_passthrough_guard)
}

/// Descends through control flow but not into nested function bodies.
fn stmt_has_passthrough_guard(stmt: &Statement) -> bool {
    match stmt {
        Statement::IfStatement(if_stmt) => {
            if_is_passthrough_guard(if_stmt)
                || stmt_has_passthrough_guard(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|alt| stmt_has_passthrough_guard(alt))
        }
        Statement::BlockStatement(block) => passthrough_guard_in_stmts(&block.body),
        Statement::TryStatement(try_stmt) => {
            passthrough_guard_in_stmts(&try_stmt.block.body)
                || try_stmt
                    .handler
                    .as_ref()
                    .is_some_and(|h| passthrough_guard_in_stmts(&h.body.body))
                || try_stmt
                    .finalizer
                    .as_ref()
                    .is_some_and(|f| passthrough_guard_in_stmts(&f.body))
        }
        Statement::ForStatement(f) => stmt_has_passthrough_guard(&f.body),
        Statement::ForInStatement(f) => stmt_has_passthrough_guard(&f.body),
        Statement::ForOfStatement(f) => stmt_has_passthrough_guard(&f.body),
        Statement::WhileStatement(w) => stmt_has_passthrough_guard(&w.body),
        Statement::DoWhileStatement(d) => stmt_has_passthrough_guard(&d.body),
        Statement::SwitchStatement(switch) => switch
            .cases
            .iter()
            .any(|case| passthrough_guard_in_stmts(&case.consequent)),
        Statement::LabeledStatement(l) => stmt_has_passthrough_guard(&l.body),
        _ => false,
    }
}

/// Does this `if` implement the `<X> instanceof Promise` sync/async passthrough?
/// The consequent (promise branch) must return a `.then`/`.catch`/`.finally`
/// chain on `<X>`, and the `else` branch must return the raw `<X>`.
fn if_is_passthrough_guard(if_stmt: &IfStatement) -> bool {
    let Some(binding) = instanceof_promise_operand(&if_stmt.test) else {
        return false;
    };
    let Some(alternate) = &if_stmt.alternate else {
        return false;
    };
    branch_returns_promise_chain_on(&if_stmt.consequent, binding)
        && branch_returns_identifier(alternate, binding)
}

/// Extract `<X>` from a `<X> instanceof Promise` test: a `BinaryExpression` whose
/// operator is `instanceof`, whose right operand is the identifier `Promise`, and
/// whose left operand is a plain identifier (the discriminated binding).
fn instanceof_promise_operand<'a>(test: &'a Expression<'a>) -> Option<&'a str> {
    let Expression::BinaryExpression(bin) = test else {
        return None;
    };
    if bin.operator != BinaryOperator::Instanceof {
        return None;
    }
    let Expression::Identifier(right) = &bin.right else {
        return None;
    };
    if right.name.as_str() != "Promise" {
        return None;
    }
    match &bin.left {
        Expression::Identifier(left) => Some(left.name.as_str()),
        _ => None,
    }
}

/// Does this branch return a `.then`/`.catch`/`.finally` call whose receiver is
/// the identifier `name`?
fn branch_returns_promise_chain_on(stmt: &Statement, name: &str) -> bool {
    match stmt {
        Statement::ReturnStatement(ret) => ret
            .argument
            .as_ref()
            .is_some_and(|arg| is_promise_chain_on(arg, name)),
        Statement::BlockStatement(block) => block
            .body
            .iter()
            .any(|s| branch_returns_promise_chain_on(s, name)),
        _ => false,
    }
}

fn is_promise_chain_on(expr: &Expression, name: &str) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "then" | "catch" | "finally") {
        return false;
    }
    matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == name)
}

/// Does this branch return the raw identifier `name`?
fn branch_returns_identifier(stmt: &Statement, name: &str) -> bool {
    match stmt {
        Statement::ReturnStatement(ret) => {
            matches!(&ret.argument, Some(Expression::Identifier(id)) if id.name.as_str() == name)
        }
        Statement::BlockStatement(block) => block
            .body
            .iter()
            .any(|s| branch_returns_identifier(s, name)),
        _ => false,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_mixed_without_explicit_union_annotation() {
        // Negative-space guard: a genuinely inconsistent async function with no
        // explicit `T | Promise<T>` return type still fires.
        let src = "function f(x: boolean) { if (x) return 1; return Promise.resolve(2); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_explicit_void_promise_union_return_type() {
        // Regression for #1619: Astro's `renderChild` declares its mixed-return
        // contract via `void | Promise<void>`, so the fast sync path is
        // intentional, not a mistake.
        let src = "function renderChild(destination: D, child: any): void | Promise<void> {
            if (typeof child === 'string') { destination.write(child); return; }
            if (isPromise(child)) { return child.then((x) => renderChild(destination, x)); }
            destination.write(child);
            return;
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_generic_union_return_type() {
        let src = "function f<T>(x: boolean, v: T): T | Promise<T> { if (x) return v; return Promise.resolve(v); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_explicit_union_return_type_on_arrow() {
        let src = "const f = (x: boolean): void | Promise<void> => { if (x) { return; } return Promise.resolve(); };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mixed_when_annotation_is_not_a_promise_union() {
        // A non-union (or non-Promise-bearing) annotation does not document a
        // mixed-return contract, so the inconsistency still fires.
        let src = "function f(x: boolean): number { if (x) return 1; return Promise.resolve(2); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_plain_promise_annotation_with_coalesce_branch() {
        // Regression for #3858: TanStack/query `runNext` is annotated
        // `Promise<unknown>`, so every branch is contractually a Promise even
        // though `foundMutation?.continue() ?? Promise.resolve()` is not
        // syntactically recognized as one.
        let src = "function runNext(s: string): Promise<unknown> {
            if (typeof s === 'string') {
                return foundMutation?.continue() ?? Promise.resolve();
            } else {
                return Promise.resolve();
            }
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_plain_promise_annotation_with_method_call_branch() {
        // Regression for #3858: `ensureQueryData` returns a method call
        // (`this.fetchQuery(options)`) in one branch and `Promise.resolve` in
        // another, both contractually `Promise<TData>`.
        let src = "function ensureQueryData(options: any): Promise<TData> {
            if (cachedData === undefined) {
                return this.fetchQuery(options);
            }
            return Promise.resolve(cachedData);
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_promise_branch_alongside_promise_resolve() {
        // Regression for #6390: nanostores' `allTasks` returns `Promise.resolve()`
        // in the fast path and `new Promise(...)` in the slow path — both branches
        // are Promises, so there is nothing to unify.
        let src = "function allTasks() {
            if (tasks === 0) {
                return Promise.resolve();
            } else {
                return new Promise(resolve => { resolves.push(resolve); });
            }
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_new_non_promise_construction_with_promise_branch() {
        // A `NewExpression` whose callee is not `Promise` (`new Date()`) is sync,
        // so mixing it with a Promise branch still fires — the new arm is scoped
        // to the global `Promise` constructor, not any constructor call.
        let src = "function f(x: boolean) { if (x) return new Date(); return Promise.resolve(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_unannotated_arrow_with_coalesce_promise_branch() {
        // Regression for #3858 (part 2): an un-annotated arrow whose branches
        // are `x ?? Promise.resolve()` and `Promise.resolve()` is all-Promise.
        let src = "const f = (x: boolean) => { if (x) { return foo() ?? Promise.resolve(); } return Promise.resolve(); };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_returntype_of_generic_type_parameter() {
        // Regression for #6471: unjs/hookable's `callHookWith` is annotated
        // `ReturnType<CallFunction>` where `CallFunction` is a generic type
        // parameter. The return type defers entirely to the generic caller, so a
        // Promise branch (`result.finally(...)`) alongside a sync branch
        // (`return result;`) mirrors the contract just like `T | Promise<T>`.
        let src = "function callHookWith<CallFunction extends (...args: any[]) => any>(caller: CallFunction): ReturnType<CallFunction> {
            const result = caller();
            if ((result as any) instanceof Promise) {
                return result.finally(() => {});
            }
            return result;
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_callback_promise_dual_mode_truthy_guard() {
        // Regression for #6826: fastify's `inject(opts, cb)` is a callback/promise
        // dual-mode API. When `cb` is supplied the result is delivered by calling
        // it (callback mode, sync/void return); when omitted a Promise is
        // returned. The mixed returns are gated on the callback-presence check.
        let src = "function inject(opts, cb) {
            if (cb) {
                cb(error);
                return lightMyRequest(httpHandler, opts, cb);
            } else {
                return Promise.reject(error);
            }
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_callback_promise_dual_mode_typeof_guard() {
        // Regression for #6826: fastify's `listen(listenOptions, cb)` checks the
        // callback with `typeof cb === 'function'` (present branch invokes `cb`)
        // and returns a Promise in the `cb === undefined` branch.
        // The present branch returns a plain sync value so its return classifies
        // as Sync and the callback-presence guard is actually exercised (a bare
        // unresolvable identifier would be Unknown and skip the guard entirely).
        let src = "function listen(listenOptions, cb = undefined) {
            if (typeof cb === 'function') {
                cb(null, server);
                return 0;
            }
            if (cb === undefined) {
                return listenPromise.then(address => address);
            }
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_callback_dual_mode_with_inequality_guard_and_call_method() {
        // The discriminator generalises across guard forms (`cb !== undefined`)
        // and invocation forms (`cb.call(...)`), not just `if (cb)` and `cb(...)`.
        // The present branch returns a plain sync value so its return classifies
        // as Sync and the callback-presence guard is actually exercised.
        let src = "function once(cb) {
            if (cb !== undefined) {
                cb.call(this, value);
                return 0;
            }
            return Promise.resolve(value);
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_callback_param_tested_but_never_invoked() {
        // Negative control for #6826: a parameter tested for presence but never
        // invoked is not a real callback, so the mixed return still fires.
        let src = "function f(opts, cb) {
            if (cb) {
                return doSync(opts);
            }
            return Promise.resolve();
        }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_mixed_gated_on_business_condition_even_when_callback_invoked() {
        // Negative control for #6826: the gate must be the callback-presence
        // check itself. Here the branch is selected by a business condition
        // (`state.ready`), not by whether `cb` was supplied, so it still fires
        // even though `cb` is invoked in the branch.
        let src = "function f(state, cb) {
            if (state.ready) {
                cb(1);
                return doSync();
            }
            return Promise.resolve();
        }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_returntype_of_concrete_typeof() {
        // Negative control for #6471: the discriminator is structural, not a
        // blanket `ReturnType` name suppression. `ReturnType<typeof concreteFn>`
        // captures a concrete function's return (the type argument is not a type
        // parameter), so a genuinely mixed body still fires.
        let src = "function f(x: boolean): ReturnType<typeof concreteFn> { if (x) return 1; return Promise.resolve(2); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_instanceof_promise_passthrough_with_finally() {
        // Regression for #7456: Budibase's `withEnv` is an isomorphic sync/async
        // wrapper. It inspects the runtime value returned by its callback: when
        // `result instanceof Promise` it chains cleanup via `.finally(cleanup)`,
        // otherwise it returns the raw sync value. Both branches forward the same
        // binding `result`, so the mixed returns mirror the callback, not a bug.
        let src = "export function withEnv<T>(v: any, f: () => T) {
            const cleanup = setEnv(v);
            const result = f();
            if (result instanceof Promise) {
                return result.finally(cleanup);
            } else {
                cleanup();
                return result;
            }
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_instanceof_promise_passthrough_with_then() {
        // The discriminator generalises across chain methods: the promise branch
        // may return `.then`/`.catch`/`.finally` on the discriminated binding.
        let src = "function w<T>(f: () => T) {
            const r = f();
            if (r instanceof Promise) {
                return r.then(x => x);
            } else {
                return r;
            }
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mixed_gated_on_business_condition_without_instanceof_promise() {
        // Negative control for #7456: the gate must be the `instanceof Promise`
        // runtime discriminator. A genuine mixed Sync/Promise return selected by a
        // business condition (`cond`) is the real footgun and still fires.
        let src = "function g(cond: boolean) { if (cond) { return Promise.resolve(1); } else { return 2; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_instanceof_promise_when_branches_forward_different_bindings() {
        // Negative control for #7456: the guard keys on the *shared* returned
        // binding. Here the chain is on `b` while the test discriminates `a` and
        // the sync branch returns a plain value `2`, so the branches do not
        // passthrough a single value and the mixed return still fires.
        let src = "function h(a: any, b: any) { if (a instanceof Promise) { return b.finally(cleanup); } else { return 2; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_sync_value_with_promise_reject_error_channel() {
        // Regression for #7519 — zxwk1998/vue-admin-better axios response
        // interceptor. `Promise.reject(...)` is the rejection/error channel
        // (equivalent to `throw`), not a resolvable promise value. The happy path
        // returns the plain `data` value while the error paths reject, which is
        // consistent under `await fn()` (returns `data` or throws), so the arrow
        // is not a real sync/promise mix.
        let src = "const interceptor = (response) => {
            const { data } = response;
            if (data === undefined || data === null) {
                return Promise.reject('backend returned empty');
            }
            if (code !== null && arr.includes(code)) {
                return data;
            } else {
                handleCode(code, msg);
                return Promise.reject(`request error`);
            }
        };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_function_decl_sync_value_with_promise_reject() {
        // Function-declaration form of #7519: a plain value on the happy path and
        // `Promise.reject(...)` on the error path is not a mix.
        let src = "function f(c: boolean) { if (!c) return Promise.reject('bad'); return 1; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_resolve_and_reject_all_async() {
        // `Promise.reject(...)` must count as the error channel, not as sync:
        // an all-async function that resolves in one branch and rejects in
        // another is uniform (no sync branch) and must stay unflagged. This
        // guards against naively reclassifying reject as `Sync`, which would
        // wrongly turn this into a sync/promise mix.
        let src = "function f(x: boolean) { if (x) return Promise.resolve(1); return Promise.reject('e'); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_sync_value_mixed_with_promise_resolve() {
        // Control for #7519: `Promise.resolve(...)` is a real resolvable promise
        // value, so mixing it with a sync return still fires.
        let src = "function f(x: boolean) { if (x) return 1; return Promise.resolve(2); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_sync_value_mixed_with_promise_all() {
        // Control for #7519: only `reject` was removed from the combinator set;
        // `Promise.all(...)` stays a promise value, so a sync branch mixed with
        // it still fires.
        let src = "function f(x: boolean) { if (x) return 1; return Promise.all([a, b]); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_promise_reject_mixed_with_real_promise_value() {
        // A `Promise.reject(...)` error branch does not launder a genuine
        // sync/promise mix: a sync `return 1` plus a resolvable
        // `Promise.resolve(2)` still fires even when a reject branch is present.
        let src = "function f(a: number) { if (a === 0) return Promise.reject('x'); if (a === 1) return 1; return Promise.resolve(2); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_unannotated_method_returning_promise_bound_identifier() {
        // Regression for #7490 — toeverything/AFFiNE `context-loader.ts`. A
        // non-annotated method returns `Promise.resolve(cached)` in one branch and
        // a bare `promise` in another, where `const promise = load()` and
        // `load: () => Promise<T>`. Resolving the binding classifies `promise` as a
        // promise branch, so both branches are Promises and nothing fires.
        let src = "class C { memo<T>(map: Map<string, Promise<T> | T>, key: string, load: () => Promise<T>) { const cached = map.get(key); if (cached) { return Promise.resolve(cached); } const promise = load(); map.set(key, promise); return promise; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_then_chain_bound_identifier_alongside_promise() {
        // A `const p = foo().then(...)` binding is a promise branch (a `.then`
        // chain), so returning `p` alongside `Promise.resolve(1)` is all-promise.
        let src = "function g(c: boolean) { if (c) { return Promise.resolve(1); } const p = foo().then((x) => x); return p; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_promise_bound_identifier_mixed_with_literal() {
        // Binding resolution does not launder a genuine mix: a promise-bound
        // identifier (`const p = Promise.resolve(1)`) in one branch and a literal
        // `2` in another is still a real sync/promise mix.
        let src = "function h(c: boolean) { if (c) { const p = Promise.resolve(1); return p; } return 2; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_literal_bound_identifier_mixed_with_promise() {
        // A genuine sync identifier is preserved: `const x = 5` is sync, so
        // returning `x` alongside a `Promise.resolve` branch is still a real mix.
        let src = "function j(c: boolean) { const x = 5; if (c) return Promise.resolve(1); return x; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_opaque_param_identifier_return() {
        // A bare parameter does not resolve to a local initializer, so it is
        // Unknown — evidence of neither a sync nor a promise branch. Mixed only
        // with a Promise branch there is no sync side, so this deliberately does
        // not fire (precision over recall for opaque returns).
        let src = "function k(p: any, c: boolean) { if (c) return Promise.resolve(1); return p; }";
        assert!(run(src).is_empty());
    }
}

//! node-callback-return OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, FormalParameters, IdentifierReference, PropertyKey,
    Statement, TSType, TSTypeName,
};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

const CALLBACKS: &[&str] = &["callback", "cb", "next"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["callback", "cb", "next"])
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

        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if !CALLBACKS.contains(&callee.name.as_str()) {
            return;
        }

        // A zero-argument call carries no error and no result, so there is
        // nothing for a missing `return` to drop. `next()`/`cb()` with no
        // arguments is a side-effecting continuation/notification, not a Node
        // error-first callback (`cb(err)`, `callback(err, data)`), so there's
        // no propagation hazard.
        if call.arguments.is_empty() {
            return;
        }

        // A callee whose declared type is a void-returning *visitor* — a
        // function type `(value, depth) => void` whose first parameter is not an
        // error — is invoked purely for its side effect. It carries no error and
        // no result for a missing `return` to propagate, and a `return` here
        // would abort the surrounding traversal early. This is structurally
        // distinct from a Node error-first callback (`(err, data) => void`),
        // which stays flagged because its first parameter is the error the
        // caller must short-circuit on. An untyped or unresolvable callee keeps
        // the default behavior.
        if callee_is_void_visitor(callee, semantic) {
            return;
        }

        // A Vue Router navigation guard receives its resolver as the third
        // parameter (`(to, from, next) => ...`). Vue Router ignores the guard's
        // return value when `next` is used, so a terminal `next(...)` needs no
        // `return`; it is a navigation resolver, not a Node error-first
        // callback. The guard is recognized by framework-API provenance (the
        // enclosing function is registered through a Vue Router guard API and
        // `next` is its third parameter), which leaves an Express-style `next`
        // middleware — registered through different methods — flagged.
        if is_vue_router_guard_next(callee, semantic) {
            return;
        }

        // If the call sits inside an arrow function whose body is an
        // expression (e.g. `(x) => cb(x)` or `(x) => wrap(cb(x))`), the
        // value is implicitly returned — there's no "forgot return" risk.
        if inside_implicit_return_arrow(node, semantic) {
            return;
        }

        // If a loop encloses the call before any function boundary, the call is
        // invoked once per iteration on purpose (calling all callbacks, not
        // branching). A `return` there would stop the loop after the first
        // item, dropping the rest — the opposite of the intended behavior.
        if inside_enclosing_loop(node, semantic) {
            return;
        }

        // Resolve the expression that actually consumes the call's value,
        // looking through value-forwarding wrappers (a ternary
        // consequent/alternate, a `&&`/`||`/`??` operand, or parentheses) that
        // transparently forward the callee's value to whatever consumes them —
        // so `cond ? callback(x) : y` assigned or returned counts as consumed,
        // exactly as the direct forms do. `consumed_span` is the span of the
        // outermost forwarded expression, so the argument-position checks match
        // the wrapper sitting in the argument slot, not the bare call.
        let (parent, consumed_span) = consuming_parent(node, semantic);
        match parent.kind() {
            // `return cb(err);`
            AstKind::ReturnStatement(_) => return,
            // Arrow body: `(err) => cb(err)`
            AstKind::ArrowFunctionExpression(_) => return,
            // `await callback(...)` — the call is awaited, so execution continues
            // afterwards by design (capture result, post-downstream cleanup, or the
            // Koa/Hono/Fastify "wrap" middleware pattern `await next(); <post-processing>`).
            // An awaited call is structurally not a fire-and-forget Node error-first
            // callback, so a trailing `return` is neither expected nor correct.
            AstKind::AwaitExpression(_) => return,
            // `result = callback(x)` / `const result = callback(x)` — the call's
            // return value is captured into a binding, so it is consumed, not dropped.
            // This is the opposite of the "forgot to `return`" mistake: a trailing
            // `return callback(x)` would skip any surrounding cleanup (e.g. a
            // `try/finally` that runs before the captured value is returned).
            AstKind::AssignmentExpression(_) => return,
            AstKind::VariableDeclarator(_) => return,
            // `push(callback(key))` / `new Wrapper(cb(x))` — the call's result is
            // passed as an argument to an enclosing call, so it is consumed
            // downstream, not dropped. A trailing `return` is impossible (the value
            // must flow into the outer call). Only exempt the arguments position:
            // for `callback(x)(y)` the inner call is the outer call's callee, whose
            // span matches no argument, so it stays flagged.
            AstKind::CallExpression(outer) if is_argument(consumed_span, &outer.arguments) => {
                return;
            }
            AstKind::NewExpression(outer) if is_argument(consumed_span, &outer.arguments) => {
                return;
            }
            AstKind::ExpressionStatement(expr_stmt) => {
                let grandparent = semantic.nodes().parent_node(parent.id());
                // The trailing-return exemption applies to a statement list at
                // any nesting: a `FunctionBody` directly, or a `BlockStatement`
                // nested in `if`/`for`/`try`/etc. (`oxc` names the slice
                // `statements` on the former and `body` on the latter).
                let stmts = match grandparent.kind() {
                    AstKind::FunctionBody(block) => Some(&block.statements),
                    AstKind::BlockStatement(block) => Some(&block.body),
                    _ => None,
                };
                if let Some(stmts) = stmts {
                    // Find our position in the block.
                    let our_span = expr_stmt.span;
                    let mut found_idx = None;
                    for (i, s) in stmts.iter().enumerate() {
                        if s.span() == our_span {
                            found_idx = Some(i);
                            break;
                        }
                    }
                    if let Some(idx) = found_idx {
                        // Check if the next statement is a return or throw.
                        if let Some(next) = stmts.get(idx + 1)
                            && matches!(
                                next,
                                Statement::ReturnStatement(_) | Statement::ThrowStatement(_)
                            ) {
                                return;
                            }

                        // No statement runs after the callback within its
                        // block. Exempt it when that block is in terminal
                        // position, so nothing runs after the callback on any
                        // path and a missing `return` drops nothing: directly a
                        // function body (unless that function is a callback
                        // argument, whose return value the outer call consumes),
                        // or an if/else branch whose enclosing `if` cascade is
                        // itself terminal.
                        if idx == stmts.len() - 1 {
                            let great_grandparent =
                                semantic.nodes().parent_node(grandparent.id());
                            match great_grandparent.kind() {
                                AstKind::Function(_)
                                | AstKind::ArrowFunctionExpression(_) => {
                                    if is_exemptible_function_body_owner(
                                        great_grandparent,
                                        semantic,
                                    ) {
                                        return;
                                    }
                                }
                                AstKind::IfStatement(_) => {
                                    if is_in_terminal_position(
                                        great_grandparent,
                                        semantic,
                                    ) {
                                        return;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Expected `return` with your callback function.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Return true if `call_span` matches one of the enclosing call's arguments,
/// i.e. the flagged call sits in arguments position rather than being the
/// callee. Spread arguments do not reach here (their semantic parent is the
/// `SpreadElement`, not the call), so a direct span match is exact.
fn is_argument(call_span: Span, arguments: &[Argument<'_>]) -> bool {
    arguments.iter().any(|arg| arg.span() == call_span)
}

/// True when a callback that is the last statement of `owner`'s body may be
/// exempted from the missing-`return` check. A plain `Function` returns to its
/// caller after the last statement, and an arrow does too — unless the arrow is
/// itself passed as a callback argument (or is the callee of an IIFE), where the
/// outer call consumes the arrow's return value, so a dropped return still
/// matters and the callback stays flagged.
fn is_exemptible_function_body_owner<'a>(
    owner: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    match owner.kind() {
        AstKind::Function(_) => true,
        AstKind::ArrowFunctionExpression(_) => {
            let parent = semantic.nodes().parent_node(owner.id());
            !matches!(
                parent.kind(),
                AstKind::CallExpression(_)
                    | AstKind::StaticMemberExpression(_)
                    | AstKind::ComputedMemberExpression(_)
            )
        }
        _ => false,
    }
}

/// Router instance methods that register a Vue Router navigation guard whose
/// callback receives the resolver as its third parameter.
const GUARD_REGISTRAR_METHODS: &[&str] = &["beforeEach", "beforeResolve", "beforeEnter"];

/// Object/class member keys that hold a Vue Router navigation guard: the
/// route-record `beforeEnter`, and the in-component guards.
const GUARD_MEMBER_KEYS: &[&str] =
    &["beforeEnter", "beforeRouteEnter", "beforeRouteUpdate", "beforeRouteLeave"];

/// True when `callee` is the `next` resolver of a Vue Router navigation guard.
///
/// The guard is recognized by framework-API provenance, never by the bare name
/// `next`: the callee must resolve to the third parameter of a function that is
/// registered through a Vue Router guard API — a `router.beforeEach` /
/// `.beforeResolve` / `.beforeEnter` call, a route-record `beforeEnter`
/// property, or an in-component `beforeRouteEnter` / `beforeRouteUpdate` /
/// `beforeRouteLeave` method. An Express middleware `next` (third parameter of
/// `app.use` / `app.get` / …) is registered through different methods, so it
/// stays flagged.
fn is_vue_router_guard_next<'a>(
    callee: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    third_param_owner(callee, semantic)
        .is_some_and(|owner| is_registered_router_guard(owner, semantic))
}

/// When `callee` resolves to the third parameter (index 2) of a function or
/// arrow, return that function node; otherwise `None`. A `next` bound to a
/// variable, an import, or any other parameter position yields `None`.
fn third_param_owner<'a>(
    callee: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a oxc_semantic::AstNode<'a>> {
    let ref_id = callee.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(sym_id);

    let mut param_span = None;
    for anc in std::iter::once(nodes.get_node(decl_id)).chain(nodes.ancestors(decl_id)) {
        let third_param_span = match anc.kind() {
            AstKind::FormalParameter(param) => {
                param_span = Some(param.span);
                continue;
            }
            AstKind::Function(f) => f.params.items.get(2).map(|p| p.span),
            AstKind::ArrowFunctionExpression(a) => a.params.items.get(2).map(|p| p.span),
            AstKind::Program(_) => return None,
            _ => continue,
        };
        return match (param_span, third_param_span) {
            (Some(declared), Some(third)) if declared == third => Some(anc),
            _ => None,
        };
    }
    None
}

/// True when `owner` (a function or arrow) is registered as a Vue Router
/// navigation guard: an argument to a call whose method is in
/// [`GUARD_REGISTRAR_METHODS`], or the value of an object/class member whose key
/// is in [`GUARD_MEMBER_KEYS`].
fn is_registered_router_guard<'a>(
    owner: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let owner_span = owner.kind().span();
    let parent = semantic.nodes().parent_node(owner.id());
    match parent.kind() {
        AstKind::CallExpression(call) => {
            matches!(
                &call.callee,
                Expression::StaticMemberExpression(m)
                    if GUARD_REGISTRAR_METHODS.contains(&m.property.name.as_str())
            ) && is_argument(owner_span, &call.arguments)
        }
        AstKind::ObjectProperty(prop) => property_key_is(&prop.key, GUARD_MEMBER_KEYS),
        AstKind::MethodDefinition(m) => property_key_is(&m.key, GUARD_MEMBER_KEYS),
        _ => false,
    }
}

/// True when a property/method key statically resolves to one of `names`.
fn property_key_is(key: &PropertyKey, names: &[&str]) -> bool {
    key.static_name()
        .is_some_and(|name| names.contains(&name.as_ref()))
}

/// True when `node` (a statement) is in terminal position: after it completes,
/// control leaves the enclosing function on every path, so no statement runs
/// afterward. `node` is terminal when it is the last statement of a function
/// body (that owner being exemptible), the last statement of a block that is
/// itself terminal, or a branch (consequent/alternate) of an `IfStatement` that
/// is itself terminal — walking up `if`/`else` cascades and enclosing blocks to
/// the function body. The walk stops at the function body, never crossing into
/// an outer function.
fn is_in_terminal_position<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    if parent.id() == node.id() {
        return false;
    }
    let node_span = node.kind().span();
    match parent.kind() {
        AstKind::FunctionBody(fb) => {
            fb.statements.last().is_some_and(|s| s.span() == node_span)
                && is_exemptible_function_body_owner(
                    semantic.nodes().parent_node(parent.id()),
                    semantic,
                )
        }
        AstKind::BlockStatement(bs) => {
            bs.body.last().is_some_and(|s| s.span() == node_span)
                && is_in_terminal_position(parent, semantic)
        }
        // `node` is the consequent or alternate branch of the `if`; after it
        // runs, control leaves the `if`, so the branch is terminal iff the `if`
        // is (this also threads `else if` chains: the inner `if` is the outer's
        // alternate).
        AstKind::IfStatement(_) => is_in_terminal_position(parent, semantic),
        _ => false,
    }
}

/// Resolve the expression that consumes the call's value, peeling
/// value-forwarding wrappers above `node`. A `ConditionalExpression` forwards
/// its `consequent`/`alternate`, a `LogicalExpression` (`&&`/`||`/`??`) forwards
/// either operand, and a `ParenthesizedExpression` forwards its inner
/// expression — in each, the callee's value flows through to whatever consumes
/// the wrapper, so the "value is consumed, not dropped" exemptions must see
/// through it. A ternary `test` operand or a `SequenceExpression` operand does
/// not forward, so it is not peeled. Peels iteratively for nested wrappers.
/// Returns the first non-forwarding ancestor and the span of the outermost
/// forwarded expression (the call's own span when nothing is peeled).
fn consuming_parent<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> (&'a oxc_semantic::AstNode<'a>, Span) {
    let nodes = semantic.nodes();
    let mut cur_id = node.id();
    let mut cur_span = node.kind().span();
    loop {
        let parent = nodes.parent_node(cur_id);
        if parent.id() == cur_id {
            return (parent, cur_span);
        }
        let forwards = match parent.kind() {
            AstKind::ParenthesizedExpression(_) => true,
            AstKind::ConditionalExpression(cond) => {
                cond.consequent.span() == cur_span || cond.alternate.span() == cur_span
            }
            AstKind::LogicalExpression(logical) => {
                logical.left.span() == cur_span || logical.right.span() == cur_span
            }
            _ => false,
        };
        if !forwards {
            return (parent, cur_span);
        }
        cur_id = parent.id();
        cur_span = parent.kind().span();
    }
}

/// True when `callee` resolves to a binding whose declared type is a
/// void-returning function type (`TSFunctionType`/`TSConstructorType` with a
/// `void`/`undefined` return) whose first parameter is not error-shaped — a
/// side-effecting visitor rather than a Node error-first callback.
fn callee_is_void_visitor<'a>(
    callee: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ty) = binding_type_annotation(callee, semantic) else {
        return false;
    };
    let (params, return_type) = match ty {
        TSType::TSFunctionType(f) => (&f.params, &f.return_type),
        TSType::TSConstructorType(c) => (&c.params, &c.return_type),
        _ => return false,
    };
    let returns_void = matches!(
        return_type.type_annotation,
        TSType::TSVoidKeyword(_) | TSType::TSUndefinedKeyword(_)
    );
    returns_void && !first_param_is_error_shaped(params)
}

/// The declared TypeScript type of the binding `ident` refers to, resolved
/// through its parameter or variable declaration. `None` when the binding has
/// no in-file annotation (imported, inferred, or untyped) — those keep the
/// rule's default behavior.
fn binding_type_annotation<'a>(
    ident: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a TSType<'a>> {
    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);

    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        match kind {
            AstKind::FormalParameter(param) => {
                return param.type_annotation.as_ref().map(|a| &a.type_annotation);
            }
            AstKind::VariableDeclarator(decl) => {
                return decl.type_annotation.as_ref().map(|a| &a.type_annotation);
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return None;
            }
            _ => {}
        }
    }
    None
}

/// True when the first parameter of a function type is a Node error-first
/// error: named `err`/`error` (case-insensitive) or typed `Error` /
/// `NodeJS.ErrnoException` (including nullable unions like `Error | null`).
/// Keeps error-first callbacks flagged while exempting visitors.
fn first_param_is_error_shaped(params: &FormalParameters) -> bool {
    let Some(first) = params.items.first() else {
        return false;
    };
    if let BindingPattern::BindingIdentifier(id) = &first.pattern {
        let name = id.name.as_str();
        if name.eq_ignore_ascii_case("err") || name.eq_ignore_ascii_case("error") {
            return true;
        }
    }
    first
        .type_annotation
        .as_ref()
        .is_some_and(|ann| type_is_error(&ann.type_annotation))
}

/// True when `ty` denotes the `Error` type or `NodeJS.ErrnoException`, directly
/// or as a member of a union (e.g. `Error | null`).
fn type_is_error(ty: &TSType) -> bool {
    match ty {
        TSType::TSTypeReference(r) => type_name_is_error(&r.type_name),
        TSType::TSUnionType(u) => u.types.iter().any(type_is_error),
        _ => false,
    }
}

/// True when a type name is `Error` or ends in `ErrnoException`
/// (i.e. `NodeJS.ErrnoException`).
fn type_name_is_error(name: &TSTypeName) -> bool {
    match name {
        TSTypeName::IdentifierReference(id) => id.name == "Error",
        TSTypeName::QualifiedName(q) => q.right.name == "ErrnoException",
        TSTypeName::ThisExpression(_) => false,
    }
}

/// Walk up from `node`; return true if a loop statement (`for`, `for...of`,
/// `for...in`, `while`, `do...while`) encloses the call before any function
/// boundary is crossed. The walk stops at the first `Function`, arrow, or
/// `Program`, so a loop in an outer function does not exempt a call sitting in
/// an inner function (e.g. a callback called inside a nested `forEach` arrow).
fn inside_enclosing_loop<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut cur = node;
    loop {
        let p = semantic.nodes().parent_node(cur.id());
        if p.id() == cur.id() {
            return false;
        }
        match p.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,
            // Function boundary: a loop above this point belongs to an outer
            // function and must not exempt this call.
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => {}
        }
        cur = p;
    }
}

/// Walk up from `node`; return true only if we reach an
/// `ArrowFunctionExpression` with `expression: true` (implicit-return arrow)
/// without crossing any scope boundary.
///
/// OXC's semantic parent chain can skip intermediate AST nodes: for
/// `(x) => outer((y) => { cb(y); })` the inner `ArrowFunctionExpression`
/// (expression: false) does not always appear as a semantic parent. Its
/// `FunctionBody` ends up with the outer arrow as its semantic parent.
/// We handle this by resolving `FunctionBody` eagerly: when we see a
/// `FunctionBody` node we immediately check its parent. If that parent is
/// `ArrowFunctionExpression(expression: false)` we stop (block-body scope).
/// If it is `ArrowFunctionExpression(expression: true)` but the current
/// `FunctionBody` does not match the arrow's own body span, we know a
/// block-body arrow was elided and stop. Otherwise we continue.
fn inside_implicit_return_arrow<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut cur = node;
    loop {
        let p = semantic.nodes().parent_node(cur.id());
        if p.id() == cur.id() {
            return false;
        }
        match p.kind() {
            AstKind::ArrowFunctionExpression(arrow) => return arrow.expression,
            // FunctionBody appears both as the synthetic wrapper for an
            // expression-body arrow and as the real block body. Resolve it
            // eagerly by peeking at its own parent.
            AstKind::FunctionBody(fb) => {
                let pp = semantic.nodes().parent_node(p.id());
                if pp.id() == p.id() {
                    return false;
                }
                match pp.kind() {
                    AstKind::ArrowFunctionExpression(arrow) => {
                        if !arrow.expression {
                            // Real block-body arrow owns this FunctionBody.
                            return false;
                        }
                        // Expression-body arrow: verify this FunctionBody is
                        // the arrow's own synthetic body and not one from a
                        // skipped inner block-body arrow.
                        if arrow.body.span() != fb.span() {
                            return false;
                        }
                        // The FunctionBody belongs to this implicit-return arrow.
                        return true;
                    }
                    AstKind::Function(_) => return false,
                    _ => return false,
                }
            }
            // ExpressionStatement wraps the synthetic statement OXC creates
            // around an arrow's expression body — transparent.
            AstKind::ExpressionStatement(_) => {}
            // Any non-arrow function, block, or statement — stop.
            AstKind::Function(_)
            | AstKind::Program(_)
            | AstKind::BlockStatement(_)
            | AstKind::ReturnStatement(_)
            | AstKind::IfStatement(_)
            | AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::SwitchStatement(_)
            | AstKind::TryStatement(_)
            | AstKind::ThrowStatement(_)
            | AstKind::LabeledStatement(_)
            | AstKind::VariableDeclaration(_) => return false,
            // Expression wrappers — transparent.
            AstKind::CallExpression(_)
            | AstKind::StaticMemberExpression(_)
            | AstKind::ComputedMemberExpression(_)
            | AstKind::SequenceExpression(_)
            | AstKind::ParenthesizedExpression(_)
            | AstKind::ConditionalExpression(_)
            | AstKind::BinaryExpression(_)
            | AstKind::LogicalExpression(_)
            | AstKind::UnaryExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSTypeAssertion(_)
            | AstKind::TSNonNullExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::ArrayExpression(_)
            | AstKind::ObjectExpression(_)
            | AstKind::ObjectProperty(_) => {}
            // Default: stop. Anything not in the transparent list could
            // introduce a new scope or statement boundary.
            _ => return false,
        }
        cur = p;
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_cb_without_return() {
        let src = "function handle(err) { if (err) { cb(err); } doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_return_cb() {
        let src = "function handle(err) { if (err) { return cb(err); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_cb_as_last_in_function() {
        let src = "function handle(err) { cb(err); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arrow_implicit_return_callback() {
        // Issue #157: arrow with expression body — `next(search)` is implicitly returned.
        let src = r#"
            const middlewares = [
                ({ search, next }) => stripDefaults(next(search), defaults),
            ];
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arrow_implicit_return_direct_callback() {
        let src = "const fn = (err) => cb(err);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_call_in_block_arrow_inside_implicit_arrow() {
        // Regression: cb inside a block-body arrow that is itself an argument
        // to a call inside an implicit-return arrow must still be flagged.
        let src = "const outer = (x) => inner((y) => { cb(y); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_when_callback_result_is_awaited_and_captured() {
        // Regression #547: `const result = await callback(conn)` followed by cleanup
        // before `return result` must not be flagged.
        let src = r#"
            async function wrap(callback) {
              try {
                const result = await callback(conn);
                await conn.unsafe("RELEASE SAVEPOINT sp");
                return result;
              } catch (err) {
                await conn.unsafe("ROLLBACK TO SAVEPOINT sp");
                throw err;
              }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_return_await_callback() {
        // `return await callback(...)` — explicitly returned, not a Node FP.
        let src = "async function wrap(callback) { return await callback(conn); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_awaited_next_then_post_processing() {
        // Issue #1220: Koa/Hono/Fastify "wrap" middleware awaits the downstream
        // chain then post-processes the response. `await next()` followed by more
        // statements is intentional — `return next()` would skip the post-processing.
        let src = r#"
            app.use('/favicon-notfound.ico', async (c, next) => {
              await next()
              c.header('X-Custom', 'Deno')
            })
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_fire_and_forget_callback() {
        // Negative space: a genuine non-awaited, non-returned Node error-first
        // callback followed by more work is still flagged.
        let src = "function f(cb) { cb(err); doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_on_zero_arg_next_continuation() {
        // Issue #3968: a Zimmerframe visitor continuation `next()` is called for
        // its side effect with zero arguments, then the function does dependent
        // work. A no-arg call propagates nothing, so a missing `return` is fine.
        let src = "function v(node, { next }) { next(); const x = []; doWork(x); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_zero_arg_cb_then_work() {
        // A bare `cb();` (no arguments) followed by more work carries no
        // error/result, so there is no propagation hazard.
        let src = "function f(cb) { cb(); doMore(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_callback_with_err_and_data() {
        let src = "function f(callback) { callback(err, data); doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_next_with_error() {
        let src = "function f(next) { next(error); doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_when_callback_result_pushed_into_array() {
        // Issue #3958: `values.push(callback(key))` — the callback's result is
        // passed as an argument to `push`, so it is consumed, not dropped. A
        // trailing `return` is impossible (the value must be pushed and iteration
        // must continue).
        let src = "objectForEachKey(obj, key => { values.push(callback(key)); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_is_call_argument() {
        // The callback's result flows as an argument into an enclosing call.
        let src = "function f(cb) { arr.push(cb(x)); doMore(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_wrapped_in_call() {
        let src = "function f(next) { wrap(next(y)); doMore(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_is_new_argument() {
        let src = "function f(callback) { const e = new Error(callback(x)); throw e; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_assigned_to_binding() {
        // Issue #3861: `result = callback(x)` captures the callback's return value
        // into a binding, then returns it after surrounding cleanup runs. A trailing
        // `return callback(x)` would skip the cleanup, so this is not a missing return.
        let src = "function run(callback) { let result; result = callback(x); return result; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_declared_into_binding() {
        // Issue #3861: `const result = callback(x)` — the result flows into a binding.
        let src = "function run(callback) { const result = callback(x); return result; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_batch_capture_with_try_finally() {
        // Issue #3861 (TanStack/query notifyManager): the callback result is captured
        // so the surrounding try/finally cleanup runs before returning it.
        let src = r#"
            const batch = (callback) => {
              let result;
              transactions++;
              try {
                result = callback(x);
              } finally {
                transactions--;
                if (!transactions) { flush(); }
              }
              return result;
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_trailing_return_in_nested_if_block() {
        // Issue #3872 (fastify lib/hooks.js): the canonical error-first escape
        // `if (err) { cb(err); return }` carries a trailing `return`, so the
        // nested `cb(err)` is not a missing-return. The exemption must apply at
        // any block nesting, not only directly in the function body.
        let src = "function handle(err, cb) { if (err) { cb(err); return; } cb(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_callback_in_nested_block_without_trailing_return() {
        // Negative space for #3872: a nested `cb(err)` with no trailing return in
        // the `if` block is still a genuine missing-return and stays flagged.
        let src = "function handle(err, cb) { if (err) { cb(err); } cb(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_callback_as_callee_of_outer_call() {
        // `cb(x)(y)` — the inner `cb(x)` is the OUTER call's callee, not an
        // argument; its result is dropped, so it stays flagged.
        let src = "function f(cb) { cb(err)(y); doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_on_callback_in_for_of_loop() {
        // Issue #5201 (unjs/hookable callEachWith): the callback is invoked once
        // per iteration to call ALL callbacks. `return callback(arg0)` would stop
        // the loop after the first, dropping the rest — the opposite of intent.
        let src = r#"
            function callEachWith(callbacks, arg0) {
              for (const callback of [...callbacks]) {
                callback(arg0);
              }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_callback_in_while_loop() {
        // A callback called inside a `while` loop body is invoked per iteration.
        let src = "function f(cb, queue) { while (queue.length) { cb(queue.pop()); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_callback_in_if_inside_function_with_outer_loop_unrelated() {
        // Negative space: a genuine branching double-call risk inside an inner
        // function must still be flagged even though an outer function has a loop.
        // The walk stops at the inner function boundary, so the outer loop does
        // not exempt this call.
        let src = r#"
            function outer(cbs) {
              for (const make of cbs) {
                make(function handle(err, cb) {
                  if (err) { cb(err); }
                  cb();
                });
              }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_on_void_visitor_callback() {
        // Issue #7261 (nestjs/nest topology-tree): a void visitor
        // `(value, depth) => void` is invoked for its side effect before the
        // walk recurses over children. It carries no error/result to propagate,
        // and a trailing `return callback(...)` would abort the traversal — this
        // is not a Node error-first callback.
        let src = r#"
            class TopologyTree {
              public walk(callback: (value: Module, depth: number) => void) {
                function walkNode(node: TreeNode<Module>, depth = 1) {
                  callback(node.value, depth);
                  node.children.forEach(child => walkNode(child, depth + 1));
                }
                walkNode(this.root);
              }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_single_param_void_visitor() {
        // A `(node) => void` visitor whose first parameter is not an error is
        // exempt: void return, non-error first parameter.
        let src = r#"
            function traverse(callback: (node: TreeNode) => void, node: TreeNode) {
              callback(node);
              node.children.forEach(c => traverse(callback, c));
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_typed_error_first_void_callback() {
        // Negative space for #7261: a void-returning callback whose first
        // parameter is an error (`(err: Error, data) => void`) is a genuine Node
        // error-first callback — the void-visitor exemption must not silence it.
        let src = r#"
            function run(callback: (err: Error, data?: string) => void) {
              if (bad) { callback(oops); }
              callback(null, "x");
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_errno_typed_first_param_not_named_err() {
        // First parameter typed `NodeJS.ErrnoException | null` marks an
        // error-first callback even when not named `err` — stays flagged.
        let src = r#"
            function run(cb: (cause: NodeJS.ErrnoException | null, data?: string) => void) {
              if (bad) { cb(oops); }
              cb(null, "x");
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_when_callback_result_assigned_via_ternary() {
        // Issue #7260 (nestjs/nest instance-wrapper): the call is the consequent
        // of a ternary whose value is assigned to a binding — the result is
        // captured and used in the following control flow, not dropped.
        let src = r#"
            function introspect(callback, dependencies, lookupRegistry) {
              let introspectionResult = dependencies
                ? callback(dependencies, lookupRegistry)
                : false;
              return introspectionResult;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_returned_via_ternary() {
        // Issue #7260: the call is the consequent of a ternary that is the
        // argument of a `return` — the result is returned, not dropped.
        let src =
            "function f(callback, enhancers, r) { return enhancers ? callback(enhancers, r) : false; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_reassigned_via_ternary() {
        // Issue #7260: `result = cond ? callback(a) : b` — the ternary forwards
        // the call's value into an assignment, so it is consumed.
        let src =
            "function f(callback, cond, a, b) { let result; result = cond ? callback(a) : b; return result; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_consumed_via_logical_operand() {
        // Issue #7260: `const y = a || callback(b)` — the `||` forwards the
        // call's value into the declaration, so it is consumed.
        let src = "function f(callback, a, b) { const y = a || callback(b); return y; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_dropped_ternary_callback() {
        // Negative space for #7260: a ternary whose value is dropped (bare
        // expression statement) still drops the callback's result — the wrapper
        // peel must not exempt it.
        let src = "function f(callback, cond, a, b) { cond ? callback(a) : b; doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_on_callback_in_terminal_if_else_cascade() {
        // Issue #7450 (element-plus / async-validator completion callback): each
        // `callback(new Error(...))` is the last statement of its if/else branch,
        // and the whole cascade is the last statement of the arrow body. Control
        // leaves the function after any branch runs, so a missing `return` drops
        // nothing.
        let src = r#"
            const validator = (rule, value, callback) => {
              if (value === "") {
                callback(new Error("required"));
              } else if (!re.test(value)) {
                callback(new Error("wrong"));
              } else {
                callback();
              }
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_callback_in_terminal_if_without_else() {
        // The `if` is the last statement of the function body, so nothing runs
        // after `cb(err)` on any path.
        let src = "function f(err, cb) { if (err) { cb(err); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_callback_when_stmt_follows_same_branch() {
        // `doMore()` follows the callback within the same branch, so the callback
        // is not the last statement of its block — a real dropped-return, still
        // flagged.
        let src = "function g(err, cb) { if (err) { cb(err); doMore(); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_on_vue_router_before_each_guard() {
        // Issue #7865 (lin-xin/vue-manage-system): `next` is the Vue Router
        // navigation-guard resolver — the third parameter of the guard passed to
        // `router.beforeEach`. The guard's return value is ignored, so a terminal
        // `next('/login')`/`next('/403')` needs no `return`.
        let src = r#"
            router.beforeEach((to, from, next) => {
              if (x) {
                next('/login');
              } else if (y) {
                next('/403');
              } else {
                next();
              }
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_vue_router_before_resolve_guard() {
        // `router.beforeResolve` is also a guard registrar; its `next` resolver
        // needs no `return`.
        let src = "router.beforeResolve((to, from, next) => { next('/x'); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_vue_router_before_enter_method_guard() {
        // `router.beforeEnter` registrar call.
        let src = "router.beforeEnter((to, from, next) => { next('/x'); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_vue_router_route_record_before_enter() {
        // Route-record `beforeEnter: (to, from, next) => ...` — `next` is the
        // guard resolver via the route-config property.
        let src = "const routes = [{ path: '/', beforeEnter: (to, from, next) => { next(false); } }];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_vue_router_in_component_guard() {
        // In-component guard `beforeRouteEnter(to, from, next) { next(vm => {}); }`
        // — `next` is the guard resolver, recognized by the method key.
        let src = "const Comp = { beforeRouteEnter(to, from, next) { next(vm => {}); } };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_vue_router_class_component_guard() {
        // In-component guard defined as a class method (vue-class-component). The
        // trailing `this.log()` makes `next(false)` non-terminal, so only the
        // guard recognition — via the `MethodDefinition` key — exempts it.
        let src = "class Comp { beforeRouteUpdate(to, from, next) { next(false); this.log(); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_express_next_middleware_with_trailing_work() {
        // Negative space for #7865: Express middleware registers through `app.use`
        // (not a Vue Router guard registrar), and `next(err)` is followed by more
        // work in the same branch — a genuine dropped-return, still flagged.
        let src = "app.use((req, res, next) => { if (err) { next(err); doMore(); } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_terminal_express_next_isolating_registrar_method() {
        // Negative space for #7865: same terminal shape as the Vue Router repro
        // (arrow is a call argument, `next` is the last statement), but `app.use`
        // is not a guard registrar — so the discriminator keys on the method, not
        // the terminal position or the name `next`. Stays flagged.
        let src = "app.use((req, res, next) => { next(err); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_node_error_first_callback_in_readfile() {
        // Negative space for #7865: a Node error-first `cb(err)` followed by more
        // work is not a Vue Router guard `next` and stays flagged.
        let src = "fs.readFile(p, (err, data) => { if (err) { cb(err); process(data); } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_local_variable_named_next() {
        // Negative space for #7865: a local `next` that is not the third parameter
        // of a Vue Router guard is not exempt — dropped result still flagged.
        let src = "function f() { const next = getNext(); next('/login'); doMore(); }";
        assert_eq!(run(src).len(), 1);
    }
}

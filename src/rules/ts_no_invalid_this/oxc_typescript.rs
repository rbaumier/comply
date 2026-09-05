//! ts-no-invalid-this OXC backend — flag `this` expressions outside
//! classes/object methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use oxc_ast::CommentKind;
use oxc_ast::ast::{AssignmentTarget, BindingPattern, Expression, TSType, TSTypeAnnotation};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True when the standalone `function` at `func_start` is preceded by a leading
/// `/** … */` JSDoc block that gives it an explicit type contract governing
/// `this` — either a `@type {…}` annotation (the function's whole signature,
/// possibly an aliased function type like `@type {Equals}`, or an inline
/// `@type {(this: T, …) => …}`) or a `@this {T}` tag. Such a function is
/// type-checked against a declared signature whose `this` binding is part of the
/// contract, so a `this` in its body is intentional, not a stray reference.
fn has_this_typed_jsdoc(
    source: &str,
    semantic: &oxc_semantic::Semantic,
    func_start: usize,
) -> bool {
    for comment in semantic.comments() {
        if comment.kind == CommentKind::Line {
            continue;
        }
        let comment_end = comment.span.end as usize;
        if comment_end > func_start {
            continue;
        }
        // Only the JSDoc block immediately preceding the function counts:
        // whitespace plus an optional `export` keyword may sit between them.
        let Some(between) = source.get(comment_end..func_start) else {
            continue;
        };
        let trimmed = between.trim();
        if !trimmed.is_empty() && trimmed != "export" && trimmed != "export default" {
            continue;
        }
        let comment_start = comment.span.start as usize;
        let Some(raw) = source.get(comment_start..comment_end) else {
            continue;
        };
        if !raw.starts_with("/**") {
            continue;
        }
        let Some(block) = scan_blocks(raw).into_iter().next() else {
            continue;
        };
        if block
            .tags()
            .iter()
            .any(|tag| tag.name == "type" || tag.name == "this")
        {
            return true;
        }
    }
    false
}

/// True when `func_id` is a `function` expression that is the right-hand side of
/// an assignment whose left-hand side is a member expression — `obj.method =
/// function () {…}` (static) or `obj[key] = function () {…}` (computed). When the
/// method is later invoked as `obj.method(...)`, `this` is bound to the receiver
/// `obj` at call time, so `this` inside the function body is the receiver and is
/// valid. This is the general method-patching (monkey-patching) idiom — e.g.
/// `md.parse = function () { return _parse.call(this, …) }` — of which the
/// `*.prototype` and `module.exports` / `exports` member assignments are special
/// cases. A function whose assignment target is a bare identifier (`f =
/// function () {…}`) has no receiver and is not matched.
fn is_method_property_assignment(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::AssignmentExpression(assign) = nodes.kind(nodes.parent_id(func_id)) else {
        return false;
    };
    matches!(
        assign.left,
        AssignmentTarget::StaticMemberExpression(_)
            | AssignmentTarget::ComputedMemberExpression(_)
    )
}

/// True when `ann` is a callable type annotation that carries its own `this`
/// contract — a named function-type alias (`TSTypeReference`, e.g.
/// `MatcherFunction<…>` / `LoadHandler`) or an inline function type
/// (`TSFunctionType`, `(this: T, …) => …`). A `function` typed against such an
/// annotation is checked against that signature's `this`, so `this` in its body
/// is the declared binding, not a stray reference.
fn is_callable_type_annotation(ann: &TSTypeAnnotation) -> bool {
    matches!(
        ann.type_annotation,
        TSType::TSTypeReference(_) | TSType::TSFunctionType(_)
    )
}

/// True when `func_id` is a `function` expression that is the initializer of a
/// variable declared with an explicit callable type annotation — either a named
/// function-type alias (`const m: MatcherFunction<…> = function () {…}`) or an
/// inline function type (`const m: (this: T, …) => … = function () {…}`). The
/// author has typed the binding as a callable, so that type — not the function
/// node's own parameter list — supplies the `this` binding; `this` in the body is
/// the declared contract, not a stray reference. (Jest/Vitest `MatcherFunction`,
/// whose signature carries a `this: MatcherContext`, is the canonical case.)
fn is_typed_callable_binding(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::VariableDeclarator(declarator) = nodes.kind(nodes.parent_id(func_id)) else {
        return false;
    };
    declarator
        .type_annotation
        .as_deref()
        .is_some_and(is_callable_type_annotation)
}

/// True when `func_id` is a `function` expression that is the value returned from
/// a function whose explicit return type is a callable type — a named
/// function-type alias (`function make(): LoadHandler { return function () {…} }`)
/// or an inline function type (`function make(): (this: T, …) => … { return
/// function () {…} }`). The enclosing function's return-type annotation is the
/// callable contract the returned function is type-checked against, and that
/// contract supplies the `this` binding, so `this` in the body is the declared
/// binding, not a stray reference. This is the return-position analog of
/// `is_typed_callable_binding` (the variable-annotation position). The function
/// must be the returned expression directly, through an optional parenthesized
/// wrapper (`return (function () {…})`); the annotation is read from the *nearest*
/// enclosing function/arrow, so a returned function whose enclosing function has
/// no return-type annotation is not exempted.
fn is_typed_callable_return(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut parent_id = nodes.parent_id(func_id);
    if matches!(nodes.kind(parent_id), AstKind::ParenthesizedExpression(_)) {
        parent_id = nodes.parent_id(parent_id);
    }
    if !matches!(nodes.kind(parent_id), AstKind::ReturnStatement(_)) {
        return false;
    }
    nodes
        .ancestors(parent_id)
        .find_map(|ancestor| match ancestor.kind() {
            AstKind::Function(func) => Some(func.return_type.as_deref()),
            AstKind::ArrowFunctionExpression(arrow) => Some(arrow.return_type.as_deref()),
            _ => None,
        })
        .flatten()
        .is_some_and(is_callable_type_annotation)
}

/// The call that receives the function at `func_id` as an argument, as the
/// `CallExpression`'s node id paired with that argument's index. Grouping and
/// assertion wrappers around the function (parentheses, `as`/`satisfies`/`<T>`,
/// `!`) are transparent, so `run(function () {…} as Hook)` reports the same
/// position as `run(function () {…})`.
///
/// `None` when the function is not an argument of a call, so a `function` in
/// callee position — an IIFE, `(function () {…})()` — never matches.
fn enclosing_call_argument(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> Option<(oxc_semantic::NodeId, usize)> {
    let nodes = semantic.nodes();
    let mut current = func_id;
    loop {
        let parent = nodes.parent_id(current);
        if parent == current {
            return None;
        }
        match nodes.kind(parent) {
            AstKind::ParenthesizedExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSNonNullExpression(_)
            | AstKind::TSTypeAssertion(_) => current = parent,
            AstKind::CallExpression(call) => {
                let arg_span = nodes.kind(current).span();
                let index = call.arguments.iter().position(|arg| arg.span() == arg_span)?;
                return Some((parent, index));
            }
            _ => return None,
        }
    }
}

/// The `CallExpression` at `call_id`, for reading back a call located by
/// [`enclosing_call_argument`].
fn call_at<'a>(
    call_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a oxc_ast::ast::CallExpression<'a>> {
    match semantic.nodes().kind(call_id) {
        AstKind::CallExpression(call) => Some(call),
        _ => None,
    }
}

/// Chai plugin-registration methods. Each invokes its registered function with
/// `this` bound to the `chai.Assertion` instance, so `this` in the function body
/// is the documented Chai plugin API.
const CHAI_REGISTRATION_METHODS: &[&str] = &[
    "addMethod", "addProperty", "overwriteMethod", "overwriteProperty",
    "addChainableMethod", "overwriteChainableMethod",
];

/// True when `expr` is a `chai.Assertion` receiver — either the bare `Assertion`
/// identifier or a member access ending in `.Assertion` (e.g. `chai.Assertion`).
/// This is the object on which Chai's plugin-registration methods are called.
fn is_chai_assertion_receiver(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(ident) => ident.name == "Assertion",
        Expression::StaticMemberExpression(member) => member.property.name == "Assertion",
        _ => false,
    }
}

/// True when `func_id` is a `function` expression passed as an argument to a Chai
/// plugin-registration call (`chai.Assertion.addMethod(name, function () {...})`,
/// `Assertion.overwriteProperty(...)`, …). Chai invokes the registered function
/// with `this` bound to the Assertion instance, so `this` inside the body is the
/// documented plugin API and is valid.
fn is_chai_registration_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some((call_id, _)) = enclosing_call_argument(func_id, semantic) else {
        return false;
    };
    let Some(call) = call_at(call_id, semantic) else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !CHAI_REGISTRATION_METHODS.contains(&member.property.name.as_str()) {
        return false;
    }
    is_chai_assertion_receiver(&member.object)
}

/// True when the reference at `ref_node_id` sits in argument position of a Chai
/// plugin-registration call (`Assertion.addChainableMethod('an', an)`). The
/// reference's nearest enclosing `CallExpression` must have a member callee whose
/// property is in `CHAI_REGISTRATION_METHODS` and whose receiver is a
/// `chai.Assertion`, and the reference itself must be inside one of that call's
/// arguments (not its callee). Chai invokes the registered function with `this`
/// bound to the Assertion instance, so passing a function's name here makes its
/// body a plugin-method body.
fn reference_is_chai_registration_arg(
    ref_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let ref_span = nodes.kind(ref_node_id).span();
    let Some(call) = nodes.ancestors(ref_node_id).find_map(|ancestor| match ancestor.kind() {
        AstKind::CallExpression(call) => Some(call),
        _ => None,
    }) else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !CHAI_REGISTRATION_METHODS.contains(&member.property.name.as_str()) {
        return false;
    }
    if !is_chai_assertion_receiver(&member.object) {
        return false;
    }
    call.arguments.iter().any(|arg| {
        let arg_span = arg.span();
        arg_span.start <= ref_span.start && ref_span.end <= arg_span.end
    })
}

/// True when the standalone named `function` at `func` is registered as a Chai
/// assertion callback by reference — its name is passed as an argument to a Chai
/// plugin-registration call (`function an() {…}` then
/// `Assertion.addChainableMethod('an', an)`). Chai invokes the registered
/// function with `this` bound to the Assertion instance, so `this` in the body is
/// the documented plugin API. This is the by-identifier registration form; the
/// inline-callback form (`Assertion.addMethod('x', function () {…})`) is handled
/// by `is_chai_registration_callback`. The function's name symbol is resolved and
/// its references enumerated via the symbol table — the same mechanism
/// `is_receiver_bound_function` uses to trace how a named function is later used.
fn is_chai_registration_callback_by_reference(
    func: &oxc_ast::ast::Function,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(id) = &func.id else {
        return false;
    };
    let Some(symbol_id) = id.symbol_id.get() else {
        return false;
    };
    semantic
        .scoping()
        .get_resolved_references(symbol_id)
        .any(|reference| reference_is_chai_registration_arg(reference.node_id(), semantic))
}

/// Node EventEmitter listener-registration methods. Each invokes its callback
/// with `this` bound to the emitter, so a non-arrow `function` callback's `this`
/// is the emitter instance at call time.
const EVENT_EMITTER_LISTENER_METHODS: &[&str] = &[
    "on", "once", "addListener", "prependListener", "prependOnceListener",
];

/// True when `expr` is a member-expression callee registering a listener — its
/// property is one of the EventEmitter listener methods (`recv.on`, `recv.once`,
/// …). This is the direct `body.on('data', function () {...})` form.
fn is_listener_method_callee(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    EVENT_EMITTER_LISTENER_METHODS.contains(&member.property.name.as_str())
}

/// True when `func_id` is a `function` expression passed as an argument to an
/// EventEmitter listener registration. Node binds `this` to the emitter inside
/// such callbacks, so `this` in the body is the emitter instance.
///
/// Two callee shapes register a listener. The direct member call
/// (`body.on('data', function () {...})`) has a callee `<recv>.<method>` whose
/// `<method>` is a listener method. The `Function.prototype` reflection form
/// (`EE.prototype.on.call(body, 'end', function () {...})` / `.apply(...)`) has a
/// callee `<member>.call` / `<member>.apply` whose `<member>` is itself a
/// listener-method member access.
///
/// The `function` must be an argument of the call (not its callee), so a bare
/// `function () { this.x }` outside such a call still flags.
fn is_event_emitter_listener_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some((call_id, _)) = enclosing_call_argument(func_id, semantic) else {
        return false;
    };
    let Some(call) = call_at(call_id, semantic) else {
        return false;
    };
    if is_listener_method_callee(&call.callee) {
        return true;
    }
    // `<member>.call(receiver, …)` / `<member>.apply(...)` reflection form.
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "call" | "apply") {
        return false;
    }
    is_listener_method_callee(&member.object)
}

/// The number of `type A = B` hops followed when reading a callable contract. A
/// cyclic alias is invalid TypeScript but reachable input, so the walk is bound.
const MAX_ALIAS_HOPS: usize = 8;

/// The declaration node the identifier `ident` resolves to, via its own symbol —
/// so a same-named binding in another scope cannot answer for it. `None` when the
/// identifier resolves to nothing (an ambient global, declared in a `.d.ts` this
/// file does not reach).
fn resolved_declaration<'a>(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<AstKind<'a>> {
    let scoping = semantic.scoping();
    let symbol_id = scoping.get_reference(ident.reference_id.get()?).symbol_id()?;
    Some(semantic.nodes().kind(scoping.symbol_declaration(symbol_id)))
}

/// The right-hand side of the `type A = …` declaration that `ty` names. `None`
/// for anything else — an interface, an imported type, a generic parameter —
/// which leaves the contract unreadable from this file.
fn alias_target<'a>(
    ty: &TSType<'a>,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a TSType<'a>> {
    let TSType::TSTypeReference(reference) = ty else {
        return None;
    };
    let oxc_ast::ast::TSTypeName::IdentifierReference(ident) = &reference.type_name else {
        return None;
    };
    let AstKind::TSTypeAliasDeclaration(decl) = resolved_declaration(ident, semantic)? else {
        return None;
    };
    Some(&decl.type_annotation)
}

/// Follow `type A = B` declarations from `ty` to the type it ultimately names,
/// returning `ty` unchanged when the chain ends or the hop bound is spent.
fn resolve_alias<'r, 'a: 'r>(
    mut ty: &'r TSType<'a>,
    semantic: &oxc_semantic::Semantic<'a>,
) -> &'r TSType<'a> {
    for _ in 0..MAX_ALIAS_HOPS {
        let Some(target) = alias_target(ty, semantic) else {
            return ty;
        };
        ty = target;
    }
    ty
}

/// Whether the callable type `ty` declares a `this` parameter — the contract a
/// `function` written against it is type-checked by. `Some(true)` for a signature
/// carrying one (`(this: Ctx) => void`), `Some(false)` for a signature without,
/// `None` when `ty` is no callable signature this file can read: an interface or
/// imported alias, a union, `any`.
fn callable_declares_this<'a>(
    ty: &TSType<'a>,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<bool> {
    match resolve_alias(ty, semantic) {
        TSType::TSFunctionType(func_type) => Some(func_type.this_param.is_some()),
        TSType::TSTypeLiteral(literal) => literal.members.iter().find_map(|member| match member {
            oxc_ast::ast::TSSignature::TSCallSignatureDeclaration(sig) => {
                Some(sig.this_param.is_some())
            }
            _ => None,
        }),
        _ => None,
    }
}

/// The `this` contract that the callable type `ty` puts on its parameter at
/// `index` — [`callable_declares_this`] of that parameter's declared type.
fn signature_parameter_contract<'a>(
    ty: &TSType<'a>,
    index: usize,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<bool> {
    match resolve_alias(ty, semantic) {
        TSType::TSFunctionType(func_type) => {
            callable_declares_this(parameter_type(&func_type.params, index)?, semantic)
        }
        TSType::TSTypeLiteral(literal) => literal.members.iter().find_map(|member| match member {
            oxc_ast::ast::TSSignature::TSCallSignatureDeclaration(sig) => {
                callable_declares_this(parameter_type(&sig.params, index)?, semantic)
            }
            _ => None,
        }),
        _ => None,
    }
}

/// The `this` contract that member `name` of the object type `ty` puts on its
/// parameter at `index`, covering both spellings of a callable member — the
/// property signature (`{ on: (e: string, fn: Listener) => void }`) and the
/// method signature (`{ on(e: string, fn: Listener): void }`).
fn member_parameter_contract<'a>(
    ty: &TSType<'a>,
    name: &str,
    index: usize,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<bool> {
    let TSType::TSTypeLiteral(literal) = resolve_alias(ty, semantic) else {
        return None;
    };
    literal.members.iter().find_map(|member| match member {
        oxc_ast::ast::TSSignature::TSPropertySignature(prop)
            if prop.key.static_name().as_deref() == Some(name) =>
        {
            signature_parameter_contract(
                &prop.type_annotation.as_ref()?.type_annotation,
                index,
                semantic,
            )
        }
        oxc_ast::ast::TSSignature::TSMethodSignature(method)
            if method.key.static_name().as_deref() == Some(name) =>
        {
            callable_declares_this(parameter_type(&method.params, index)?, semantic)
        }
        _ => None,
    })
}

/// The declared type of parameter `index` of `params`.
fn parameter_type<'r, 'a>(
    params: &'r oxc_ast::ast::FormalParameters<'a>,
    index: usize,
) -> Option<&'r TSType<'a>> {
    Some(&params.items.get(index)?.type_annotation.as_ref()?.type_annotation)
}

/// The `this` contract the callee of `call` declares for its parameter at
/// `index` — the third place a callable contract reaches a `function`, after the
/// variable annotation ([`is_typed_callable_binding`]) and the return-type
/// annotation ([`is_typed_callable_return`]).
///
/// `Some(true)` when the parameter's declared type carries a `this` parameter, so
/// TypeScript types the argument's `this` as that context; `Some(false)` when the
/// parameter is a callable type declaring none, so the argument's `this` is
/// genuinely unbound; `None` when the callee's signature is not readable from
/// this file — it is imported, ambient, or annotated with a type this analysis
/// does not resolve — and the callee's published contract answers instead.
///
/// The callee is read from its declaration: a `function`/`declare function`
/// statement, a binding annotated with a callable type, or a member of a binding
/// annotated with an object type (`declare const emitter: { on: (…) => void }`).
fn callee_parameter_contract<'a>(
    call: &oxc_ast::ast::CallExpression<'a>,
    index: usize,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<bool> {
    match &call.callee {
        Expression::Identifier(callee) => match resolved_declaration(callee, semantic)? {
            AstKind::Function(func) => {
                callable_declares_this(parameter_type(&func.params, index)?, semantic)
            }
            AstKind::VariableDeclarator(declarator) => signature_parameter_contract(
                &declarator.type_annotation.as_ref()?.type_annotation,
                index,
                semantic,
            ),
            _ => None,
        },
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(object) = &member.object else {
                return None;
            };
            let AstKind::VariableDeclarator(declarator) = resolved_declaration(object, semantic)?
            else {
                return None;
            };
            member_parameter_contract(
                &declarator.type_annotation.as_ref()?.type_annotation,
                member.property.name.as_str(),
                index,
                semantic,
            )
        }
        _ => None,
    }
}

/// True when `arg` is a primitive-literal call argument — a number, string,
/// template string, boolean, `null`, bigint, or regexp literal. A primitive
/// cannot serve as a meaningful `this` receiver, so a primitive sitting after a
/// callback is ordinary data (e.g. `setTimeout(fn, 100)`'s delay), not a
/// `thisArg`.
fn is_primitive_literal_arg(arg: &oxc_ast::ast::Argument) -> bool {
    matches!(
        arg,
        oxc_ast::ast::Argument::NumericLiteral(_)
            | oxc_ast::ast::Argument::StringLiteral(_)
            | oxc_ast::ast::Argument::TemplateLiteral(_)
            | oxc_ast::ast::Argument::BooleanLiteral(_)
            | oxc_ast::ast::Argument::NullLiteral(_)
            | oxc_ast::ast::Argument::BigIntLiteral(_)
            | oxc_ast::ast::Argument::RegExpLiteral(_)
    )
}

/// True when `func_id` is a non-arrow `function` passed as a callback argument to
/// a `CallExpression` that hands it a receiver through a sibling argument, so
/// `this` in the callback body is that bound receiver, not unbound. Two argument
/// shapes supply the receiver:
///
/// - **Leading `this`** (`Effect.gen(this, function* () {…})`, `STM.gen(this,
///   function* () {…})`, `Layer.scopedContext(Effect.gen(this, function* () {…}))`):
///   a direct `this` argument *before* the callback. Effect's `gen(self, body)`
///   declares the body as `(this: Self) => Generator<…>` and invokes it as
///   `body.call(self)`, binding the callback's `this` to that first argument.
/// - **Trailing `thisArg`** (`arr.map(function () {…}, this)`, `arr.forEach(fn,
///   thisArg)`, `of(…).pipe(every(fn, thisArg))`): the argument immediately
///   *after* the callback — the ECMAScript `thisArg` convention shared by
///   `Array.prototype.{map,forEach,…}`, RxJS predicate operators, and
///   `(collection, callback, context)` util libraries (zrender, lodash). It binds
///   `this` whether it is the literal `this` keyword, a local variable, or an
///   object literal, but not a primitive literal, which cannot be a receiver
///   (`setTimeout(fn, 100)`'s trailing `100` is a delay, so its `this` stays
///   unbound).
///
/// A leading argument that is not a direct `this` (`foo(bar(this), fn)`, where the
/// `this` is nested in a sub-expression) is data, not a receiver, so the
/// callback's `this` stays unbound.
fn is_callback_with_sibling_this_arg(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some((call_id, callback_index)) = enclosing_call_argument(func_id, semantic) else {
        return false;
    };
    let Some(call) = call_at(call_id, semantic) else {
        return false;
    };
    // Leading `this` argument (the `Effect.gen(this, fn)` receiver-binding form).
    if call.arguments[..callback_index]
        .iter()
        .any(|arg| matches!(arg, oxc_ast::ast::Argument::ThisExpression(_)))
    {
        return true;
    }
    // Trailing `thisArg` immediately after the callback, unless it is a
    // primitive literal that cannot serve as a receiver.
    call.arguments
        .get(callback_index + 1)
        .is_some_and(|arg| !is_primitive_literal_arg(arg))
}

/// True when `call` is a `$(this)` call — a call to the bare `$` identifier
/// whose first argument is a `this` expression. This is the canonical
/// jQuery/cheerio idiom for wrapping the element the library bound to `this` in
/// an iterator callback (`$(this).attr(...)`); it only makes sense when the
/// caller has rebound `this` to the current element.
fn is_jquery_wrap_of_this(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    callee.name == "$"
        && matches!(
            call.arguments.first(),
            Some(oxc_ast::ast::Argument::ThisExpression(_))
        )
}

/// True when `func_id`'s own body contains the jQuery/cheerio `$(this)` idiom —
/// a `$(this)` call whose nearest enclosing non-arrow `function` is `func_id`
/// itself (arrows are transparent). jQuery and cheerio invoke iterator callbacks
/// (`.map`/`.each`/`.filter`/…) with `this` bound to the current element, and
/// wrapping it as `$(this)` is the documented way to read that element. Such a
/// non-arrow `function` callback has had its `this` rebound by the library, so
/// every `this` in its body is the bound element, not a stray reference.
///
/// The scan keys on the `$(this)` call specifically, so a `function` that merely
/// references `$` for something else, or uses `this` with no `$(this)` wrap, is
/// not exempted. A `$(this)` inside a *nested* function binds that inner
/// function, not this one, so it does not exempt an outer function's `this`.
/// Arrow functions never reach this check (they are transparent to the
/// `this`-boundary walk), and a top-level `this` has no enclosing `function` so
/// it is never exempted either.
fn function_body_has_jquery_this(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let func_span = nodes.kind(func_id).span();
    nodes.iter().any(|node| {
        let AstKind::CallExpression(call) = node.kind() else {
            return false;
        };
        if call.span.start < func_span.start
            || call.span.end > func_span.end
            || !is_jquery_wrap_of_this(call)
        {
            return false;
        }
        // The `$(this)` must bind `func_id` directly: its nearest enclosing
        // non-arrow `function` is `func_id`, not a nested inner function.
        nearest_non_arrow_function(node.id(), semantic) == Some(func_id)
    })
}

/// The `NodeId` of the nearest non-arrow `Function` ancestor of `node_id`, or
/// `None` if there is none before module scope. Arrow functions are transparent
/// (an arrow does not introduce a `this` binding), matching the boundary the
/// `this`-validity walk uses.
fn nearest_non_arrow_function(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    semantic
        .nodes()
        .ancestors(node_id)
        .find(|ancestor| matches!(ancestor.kind(), AstKind::Function(_)))
        .map(|ancestor| ancestor.id())
}

/// True when `name` follows the constructor-function convention: after any
/// leading underscores, the first character is an uppercase ASCII letter (e.g.
/// `Suspense`, `Component`, or the module-private `_Reply`). Such functions are
/// conventionally invoked with `new`, so `this` is the new instance. Leading
/// underscores mark a binding as internal/private and do not change the
/// capitalized-initial signal.
fn is_constructor_name(name: &str) -> bool {
    name.trim_start_matches('_')
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// True when the reference at `ref_node_id` is used in a way that binds the
/// function's `this` at call time:
/// - `new F(...)` — constructor invocation,
/// - `F.call(this, ...)` / `F.apply(...)` / `F.bind(...)` — explicit binding,
/// - `x.member = F` — assigned as a method value (receives the receiver as `this`),
/// - `{ member: F }` / `{ F }` — the object-literal form of the same method
///   slot; a later `obj.member(...)` binds `obj`. The check is positional: it
///   accepts the slot without proving such a call exists.
///
/// The property branch requires the reference to be the property *value*, so a
/// computed key (`{ [F]: 1 }`) does not match. The shorthand form (`{ F }`) gives
/// key and value the same span, so the value check accepts it.
fn reference_binds_this(
    ref_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let ref_span = nodes.kind(ref_node_id).span();
    match nodes.kind(nodes.parent_id(ref_node_id)) {
        AstKind::NewExpression(_) => true,
        AstKind::StaticMemberExpression(member) => {
            matches!(member.property.name.as_str(), "call" | "apply" | "bind")
        }
        AstKind::AssignmentExpression(assign) => {
            matches!(
                assign.left,
                AssignmentTarget::StaticMemberExpression(_)
                    | AssignmentTarget::ComputedMemberExpression(_)
            ) && assign.right.span() == ref_span
        }
        AstKind::ObjectProperty(prop) => prop.value.span() == ref_span,
        _ => false,
    }
}

/// True when the standalone `function` at `func` gets its `this` from the call
/// site — either by the PascalCase constructor-naming convention, or because its
/// name is referenced somewhere in the module in a position that supplies a
/// receiver: `new`/`.call`/`.apply`/`.bind`, a method-value assignment, or an
/// object-literal property value.
///
/// One such reference is enough. A function whose remaining references supply no
/// receiver stays exempt, because a reference position that supplies none is not
/// evidence of an unbound call: a helper handed to a callback registration, or
/// re-exported beside a local call, reaches its receiver through a slot no
/// position can show.
fn is_receiver_bound_function(
    func: &oxc_ast::ast::Function,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(id) = &func.id else {
        return false;
    };
    if is_constructor_name(&id.name) {
        return true;
    }
    let Some(symbol_id) = id.symbol_id.get() else {
        return false;
    };
    semantic
        .scoping()
        .get_resolved_references(symbol_id)
        .any(|reference| reference_binds_this(reference.node_id(), semantic))
}

/// True when `func_id` is a `function` expression that is the initializer of a
/// `const`/`let`/`var` binding whose name is referenced somewhere in the module
/// in a way that binds `this` at call time — `name.bind(this)`, `name.call(...)`,
/// `name.apply(...)`, `new name(...)`, assigned as a method value
/// (`x.member = name`), or installed as an object-literal property value
/// (`{ member: name }`). This generalizes the named-function logic in
/// `is_receiver_bound_function` to anonymous function expressions held in a variable:
/// `const localeData = function () { … this.$locale() … }` that is later invoked
/// via `localeData.bind(this)()` (the dayjs plugin / bound-method idiom) has its
/// `this` supplied at the binding site, so `this` in the body is intentional. One
/// such reference is enough, for the reason given on `is_receiver_bound_function`.
fn is_var_bound_function_referenced_for_this(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::VariableDeclarator(declarator) = nodes.kind(nodes.parent_id(func_id)) else {
        return false;
    };
    let BindingPattern::BindingIdentifier(ident) = &declarator.id else {
        return false;
    };
    let Some(symbol_id) = ident.symbol_id.get() else {
        return false;
    };
    semantic
        .scoping()
        .get_resolved_references(symbol_id)
        .any(|reference| reference_binds_this(reference.node_id(), semantic))
}

/// True when the `ThisExpression` at `this_node_id` is the second positional
/// argument of a `Reflect.apply(fn, this, args)` call. `Reflect.apply` invokes
/// `fn` with its second argument bound as the receiver, so a `this` written
/// there forwards the enclosing function's own `this` — the standard
/// context-forwarding idiom, equivalent to `fn.apply(this, args)` /
/// `fn.call(this, args)`. The callee must be the `Reflect.apply` member
/// expression (object identifier `Reflect`, property `apply`), and the
/// `ThisExpression` must be the call's `arguments[1]` directly (a `this` buried
/// in a sub-expression of the second argument is not this idiom); the call must
/// carry at least two arguments.
fn is_reflect_apply_this_arg(
    this_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::CallExpression(call) = nodes.kind(nodes.parent_id(this_node_id)) else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name != "apply"
        || !matches!(&member.object, Expression::Identifier(id) if id.name == "Reflect")
    {
        return false;
    }
    let Some(second_arg) = call.arguments.get(1) else {
        return false;
    };
    second_arg.span() == nodes.kind(this_node_id).span()
}

/// True when the node at `node_id` sits in `thisArg` position of a
/// receiver-binding invocation: the first argument of `<callee>.call(X, …)` /
/// `<callee>.apply(X, …)`, or the second argument of `Reflect.apply(fn, X, …)`.
/// Each binds its `thisArg` as the receiver of the invoked function, so the value
/// written there is handed to the callee as its `this`. The check is a property of
/// the argument node's position, so it holds for a literal `this` written there
/// (`fn.apply(this, args)`, the receiver-forwarding wrapper idiom) as much as for
/// an identifier carrying a captured `this` (`fn.apply(self, args)`). A `Reflect`
/// receiver is excluded from the first-argument branch — `Reflect.apply`'s first
/// argument is the function to invoke, not the receiver (matched at argument two).
fn is_receiver_binding_this_arg(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // `Reflect.apply(fn, X, args)`: X is the second (thisArg) argument.
    if is_reflect_apply_this_arg(node_id, semantic) {
        return true;
    }
    let nodes = semantic.nodes();
    let AstKind::CallExpression(call) = nodes.kind(nodes.parent_id(node_id)) else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "call" | "apply")
        || matches!(&member.object, Expression::Identifier(id) if id.name == "Reflect")
    {
        return false;
    }
    let node_span = nodes.kind(node_id).span();
    call.arguments
        .first()
        .is_some_and(|arg| arg.span() == node_span)
}

/// True when the `ThisExpression` at `this_node_id` is captured into a local
/// binding (`const`/`let`/`var X = this`) whose symbol is later forwarded as the
/// `thisArg` of a `<callee>.call(X, …)` / `<callee>.apply(X, …)` /
/// `Reflect.apply(fn, X, …)` invocation — the `var self = this` / `const that =
/// this` context-capture idiom. The enclosing non-arrow `function` binds `this`
/// dynamically at its call site; `this` is captured into the local and forwarded
/// to preserve that receiver (the reference may sit inside a nested closure, as in
/// `const later = function () { fn.apply(context, args); }`), so the `this` is
/// intentional. This is one indirection hop from writing `this` directly as the
/// thisArg (`is_receiver_binding_this_arg`), and like it is a property of the
/// `this` node itself. The `this` must be the whole initializer of the declarator, whose
/// bound name is a plain identifier; a `this` buried inside the initializer
/// (`const x = this.foo`) is not this idiom.
fn is_this_captured_and_forwarded_as_this_arg(
    this_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::VariableDeclarator(declarator) = nodes.kind(nodes.parent_id(this_node_id)) else {
        return false;
    };
    let Some(init) = &declarator.init else {
        return false;
    };
    if init.span() != nodes.kind(this_node_id).span() {
        return false;
    }
    let BindingPattern::BindingIdentifier(ident) = &declarator.id else {
        return false;
    };
    let Some(symbol_id) = ident.symbol_id.get() else {
        return false;
    };
    semantic
        .scoping()
        .get_resolved_references(symbol_id)
        .any(|reference| is_receiver_binding_this_arg(reference.node_id(), semantic))
}

/// Test-runner registration functions. Mocha, Jest, Jasmine and Vitest invoke a
/// non-arrow callback registered through one of these with `this` bound to the
/// suite/test context — which is why their type definitions declare the callback
/// as `(this: Context, …) => void`, and why a hook that calls `this.timeout(…)`
/// is written `function`, not `=>`.
const TEST_HOOK_REGISTRATION_FUNCTIONS: &[&str] = &[
    "describe", "context", "suite", "it", "test", "specify", "before", "after",
    "beforeEach", "afterEach", "beforeAll", "afterAll", "setup", "teardown",
    "suiteSetup", "suiteTeardown",
];

/// The identifier a callee expression is rooted at, seen through member accesses
/// and calls: `it`, `it.only`, `it.each(table)` and `describe.each(table)(…)` all
/// root at their bare name, so the member and template forms of a registration
/// need no enumeration of their own.
fn callee_root_identifier<'r, 'a>(
    expr: &'r Expression<'a>,
) -> Option<&'r oxc_ast::ast::IdentifierReference<'a>> {
    match expr {
        Expression::Identifier(ident) => Some(ident),
        Expression::StaticMemberExpression(member) => callee_root_identifier(&member.object),
        Expression::CallExpression(call) => callee_root_identifier(&call.callee),
        _ => None,
    }
}

/// True when `func_id` is a `function` passed to a test-runner registration whose
/// name is bound outside this file — an ambient global (mocha, jasmine) or an
/// import (`import { it } from 'vitest'`). The runner invokes the callback with
/// `this` bound to the suite/test context, so `this` in the body is that context.
///
/// A callee *declared in this file* is never the framework's: its own signature is
/// readable, so [`callee_parameter_contract`] answers for it instead — a local
/// `declare function before(fn: () => void)` keeps flagging its callback's `this`.
fn is_test_hook_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some((call_id, _)) = enclosing_call_argument(func_id, semantic) else {
        return false;
    };
    let Some(call) = call_at(call_id, semantic) else {
        return false;
    };
    let Some(root) = callee_root_identifier(&call.callee) else {
        return false;
    };
    if !TEST_HOOK_REGISTRATION_FUNCTIONS.contains(&root.name.as_str()) {
        return false;
    }
    !matches!(
        resolved_declaration(root, semantic),
        Some(AstKind::Function(_) | AstKind::VariableDeclarator(_))
    )
}

/// The `this` contract the callee declares for the `function` at `func_id`, when
/// that function is a call argument and the callee's signature is readable here.
fn argument_position_contract(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> Option<bool> {
    let (call_id, index) = enclosing_call_argument(func_id, semantic)?;
    callee_parameter_contract(call_at(call_id, semantic)?, index, semantic)
}

/// True when the call that receives the `function` at `func_id` as an argument
/// binds its `this`.
///
/// The callee's declared signature decides whenever this file can read it: a
/// parameter typed with a callable that carries a `this` parameter supplies the
/// binding, and one typed with a callable that declares none leaves the callback
/// genuinely unbound. It is authoritative in both directions — a signature
/// written for the callee outranks anything inferred from the callee's name.
///
/// Only when the signature is out of reach — the callee is imported or ambient —
/// does the published contract of the library it belongs to answer: Chai plugin
/// registration, EventEmitter listener registration, or a test-runner hook.
fn callee_contract_binds_this(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    match argument_position_contract(func_id, semantic) {
        Some(declares_this) => declares_this,
        None => {
            is_chai_registration_callback(func_id, semantic)
                || is_event_emitter_listener_callback(func_id, semantic)
                || is_test_hook_callback(func_id, semantic)
        }
    }
}

fn is_valid_this_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    // `fn.apply(this, args)` / `fn.call(this, …)` / `Reflect.apply(fn, this, args)`:
    // the `this` is written directly in `thisArg` position, forwarding the
    // enclosing function's receiver to `fn` — the receiver-forwarding wrapper
    // idiom. This is a property of the `this` node itself, independent of the
    // enclosing function, so it is checked before the boundary walk.
    if is_receiver_binding_this_arg(node.id(), semantic) {
        return true;
    }
    // `const context = this; … fn.apply(context, args)`: the `this` is captured
    // into a local binding that is later forwarded as the `thisArg` of a
    // `.call`/`.apply`/`Reflect.apply` invocation — the `var self = this` /
    // `const that = this` context-capture idiom, one indirection hop from writing
    // `this` directly as the thisArg. Like the `Reflect.apply` case above, this is
    // a property of the `this` node itself, so it is checked before the walk.
    if is_this_captured_and_forwarded_as_this_arg(node.id(), semantic) {
        return true;
    }
    // Walk up from the ThisExpression. The first `this`-binding boundary
    // determines validity:
    // - ArrowFunction: transparent, keep going.
    // - Function inside a MethodDefinition (class method): valid.
    // - Function that is an object-literal method or property value: valid.
    // - Standalone Function: invalid — stop.
    // - Class: valid (property initializer, etc.).
    let mut entered_function: Option<oxc_span::Span> = None;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Class(_) => return true,
            AstKind::ArrowFunctionExpression(_) => continue,
            AstKind::Function(func) => {
                // Explicit TypeScript `this` parameter: a function declaring a
                // formal `this` parameter (`function f(this: T, …) {…}`) types
                // its `this` context as part of the signature, so `this` in the
                // body is the declared binding and is valid.
                if func.this_param.is_some() {
                    return true;
                }
                // Typed callable binding: a `function` assigned to a variable
                // whose annotation is a function-type alias or inline function
                // type (`const m: MatcherFunction<…> = function () {…}`) is typed
                // against a callable contract that supplies `this`, so `this` in
                // the body is the declared binding and is valid.
                if is_typed_callable_binding(ancestor.id(), semantic) {
                    return true;
                }
                // Typed callable return position: a `function` returned from a
                // function whose explicit return type is a callable type alias
                // or inline function type (`function make(): LoadHandler {
                // return function () {…} }`) is type-checked against that
                // callable contract, which supplies `this`, so `this` in the
                // body is the declared binding and is valid. The return-position
                // analog of the typed-callable binding above.
                if is_typed_callable_return(ancestor.id(), semantic) {
                    return true;
                }
                // Method-property assignment: a function assigned to a member
                // of any object (`obj.method = function () {…}`, `obj[k] = …`)
                // is a method — when invoked as `obj.method(...)`, `this` is
                // bound to the receiver at call time, so `this` is valid. This
                // subsumes the `*.prototype` and `module.exports` patching idioms.
                if is_method_property_assignment(ancestor.id(), semantic) {
                    return true;
                }
                // Callee contract: a `function` passed as a call argument is
                // type-checked against the callee's declared parameter type, and
                // that type supplies the `this` binding when it declares a `this`
                // parameter (`declare function before(fn: (this: Ctx) => void)`).
                // Where the callee's signature is out of reach the library's
                // published contract answers instead — Chai plugin registration,
                // EventEmitter listener registration, a test-runner hook.
                if callee_contract_binds_this(ancestor.id(), semantic) {
                    return true;
                }
                // Chai registration by reference: a named function declaration
                // passed by identifier (`function an() {…};
                // Assertion.addChainableMethod('an', an)`) is invoked with `this`
                // bound to the Assertion instance like the inline form.
                if is_chai_registration_callback_by_reference(func, semantic) {
                    return true;
                }
                // Sibling-thisArg callback: a `function` passed to a call that
                // also hands it a receiver through a sibling argument — a direct
                // `this` *before* the callback (`Effect.gen(this, function* () {…})`,
                // the receiver-binding generator-adapter form) or a non-primitive
                // argument immediately *after* it (`arr.map(function () {…}, this)`,
                // `arr.forEach(fn, thisArg)`, the ECMAScript `thisArg` convention) —
                // is invoked with that receiver bound as `this`, so `this` is valid.
                if is_callback_with_sibling_this_arg(ancestor.id(), semantic) {
                    return true;
                }
                // jQuery/cheerio iterator callback: a non-arrow `function` whose
                // body wraps `this` as `$(this)` (`.map(function () { $(this) })`,
                // `.each(...)`, …) has had its `this` rebound by the library to the
                // current element, so `this` in the body is the bound element.
                if function_body_has_jquery_this(ancestor.id(), semantic) {
                    return true;
                }
                // Receiver-bound function: a PascalCase `function`, or one
                // referenced via `new`/`.call(this)`/`.apply`/`.bind`, assigned
                // as a method value, or installed as an object-literal property
                // (`const api = { inject }`), gets its receiver as `this`.
                if is_receiver_bound_function(func, semantic) {
                    return true;
                }
                // Var-bound function referenced for `this`: an anonymous
                // `function` expression held in a `const`/`let`/`var` whose
                // binding later appears in a receiver-supplying position —
                // `.bind(this)`/`.call`/`.apply`, `new`, a method-value
                // assignment, or an object-literal property — takes its `this`
                // from that site.
                if is_var_bound_function_referenced_for_this(ancestor.id(), semantic) {
                    return true;
                }
                // JSDoc `@type {…}` / `@this {…}` annotation: the function has an
                // explicit declared type contract whose `this` binding is part
                // of the signature (e.g. `/** @type {(this: T, …) => …} */` or an
                // aliased function type), so `this` in the body is intentional.
                if has_this_typed_jsdoc(source, semantic, func.span.start as usize) {
                    return true;
                }
                // Mark that we've entered a function scope; need to
                // check if it's wrapped in a MethodDefinition.
                entered_function = Some(func.span);
            }
            AstKind::MethodDefinition(_) if entered_function.is_some() => {
                // The Function was a class method — `this` is valid.
                return true;
            }
            AstKind::PropertyDefinition(_) if entered_function.is_some() => {
                // Property initializer context — valid.
                return true;
            }
            AstKind::ObjectProperty(prop)
                if entered_function.is_some_and(|func_span| {
                    prop.method || prop.value.span() == func_span
                }) =>
            {
                // Object-literal method or function-valued property —
                // `this` is bound to the object when called as `obj.key()`.
                // Both the shorthand form (`{ foo() { this } }`,
                // `prop.method == true`) and the non-shorthand form
                // (`{ foo: function () { this } }`, where the entered function
                // is exactly the property value) are valid. A function nested
                // deeper inside the value (`{ foo: arr.map(function () { this }) }`)
                // has a different value span and stays flagged.
                return true;
            }
            _ => {
                // If we already hit a standalone function (not a method),
                // any other ancestor means `this` is unbound.
                if entered_function.is_some() {
                    return false;
                }
            }
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
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ThisExpression(this_expr) = node.kind() else {
                continue;
            };

            if is_valid_this_context(node, semantic, ctx.source) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, this_expr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`this` used outside a class or valid context — likely a bug."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }

        diagnostics
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_this_at_top_level() {
        let diags = run_on("console.log(this);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_class_method() {
        assert!(run_on("class Foo { bar() { return this.x; } }").is_empty());
    }

    #[test]
    fn flags_this_in_standalone_function() {
        let diags = run_on("function foo() { return this; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_object_literal_async_iterator_method() {
        let src = "const asyncIterable = {\n  next() { return iter.next(); },\n  [Symbol.asyncIterator]() {\n    return this;\n  },\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_valued_property() {
        // A `function` expression that is the value of an object property is a
        // method — `this` is bound to the object when called as `obj.foo()`.
        assert!(run_on("const obj = { foo: function() { return this; } };").is_empty());
    }

    #[test]
    fn allows_this_in_named_function_expression_property() {
        // Regression for #1642: fastify defines public-API methods as named
        // function expressions assigned to object properties (`function _delete`)
        // for clearer stack traces; `this` is the instance at call time.
        let src = "const fastify = {\n  delete: function _delete (url, options, handler) {\n    return router.prepareRoute.call(this, { method: 'DELETE', url, options, handler })\n  },\n  hasPlugin: function (name) {\n    return this[kRegisteredPlugins].includes(name)\n  },\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_nested_in_property_value() {
        // Negative: a `function` nested inside the property value (not the value
        // itself) gets no object binding — `this` is unbound and must fire.
        let diags = run_on("const obj = { foo: arr.map(function () { return this.x; }) };");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_prototype_patch_via_alias() {
        // Regression for #2031: `proto[method] = function() { this }` where
        // `proto` is an alias of `SomeClass.prototype`.
        let src = "var proto = SvelteDate.prototype;\nproto[method] = function (...args) {\n  return this.x.apply(this, args);\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_prototype_patch_static() {
        let src = "Foo.prototype.m = function () { return this.x; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_prototype_patch_computed() {
        let src = "Foo.prototype[k] = function () { return this.x; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_method_patching_assignment() {
        // Regression for #6166: the markdown-it method-patching idiom assigns a
        // `function` to a plain object member (`md.parse = function () {…}`).
        // When invoked as `md.parse(src, env)`, `this` is bound to `md`, and
        // `_parse.call(this, …)` forwards that receiver — `this` is valid.
        let src = "const _parse = md.parse;\nmd.parse = function (src, env) {\n  return _parse.call(this, src, env);\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_computed_member_method_assignment() {
        // Regression for #6166: the computed-member form (`obj['m'] = function
        // () {…}`) binds `this` to `obj` at call time exactly like the static
        // member form, so `this` in the body is valid.
        let src = "obj['m'] = function () {\n  return this.x;\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_identifier_target_assignment() {
        // Negative-space guard for #6166: a `function` assigned to a bare
        // identifier target (`f = function () {…}`, not a member of any object)
        // has no receiver — `this` is unbound and must fire.
        let diags = run_on("let f;\nf = function () {\n  return this.x;\n};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_object_create_prototype_patch() {
        // Regression for #3386: express builds its response prototype with
        // `Object.create(SomeClass.prototype)` and assigns methods as properties.
        // The object inherits from a prototype and its methods are invoked as
        // `res.status(200)`, so `this` is the instance at call time.
        let src = "var res = Object.create(http.ServerResponse.prototype);\nres.status = function status(code) {\n  this.statusCode = code;\n  return this;\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_free_function_not_assigned_as_method() {
        // Negative-space guard for #3386: a free-floating `function` not assigned
        // as any object's method has an unbound `this` and must still fire.
        let diags = run_on("function foo() { this.x = 1; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_module_exports_namespace_method() {
        // Regression for #3643: express's `lib/application.js` exposes its public
        // object via `var app = exports = module.exports = {}` then augments it
        // (`app.init = function () { this.cache = ... }`). `app.init()` binds
        // `this` to the namespace object, so `this` is valid.
        let src = "var app = exports = module.exports = {};\napp.init = function init() {\n  this.cache = Object.create(null);\n  this.defaultConfiguration();\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_direct_module_exports_namespace_method() {
        // Regression for #3643: the shorter `var app = module.exports = {}` chain
        // is recognized the same way.
        let src = "var app = module.exports = {};\napp.foo = function () { return this.x; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_bare_exports_namespace_method() {
        // Regression for #3643: a bare `exports` chain (`var app = exports = {}`)
        // also yields the CommonJS namespace object.
        let src = "var app = exports = {};\napp.bar = function () { return this.y; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_pascal_case_constructor_function() {
        // Regression for #1916: a PascalCase `function` is a constructor function
        // by convention — called with `new`, `this` is the new instance.
        let src = "export function Suspense() {\n  this._pendingSuspensionCount = 0;\n  this._suspenders = null;\n  this._detachOnNextRender = null;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_constructor_function_calling_super_via_call() {
        // Regression for #1916: prototype-based inheritance — the PascalCase
        // constructor uses `.call(this, ...)` and assigns `this.*`.
        let src = "export function Component(props, context) {\n  CevicheComponent.call(this, props, context);\n  const render = this.render;\n  this.render = function () {};\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_assigned_as_method() {
        // Regression for #1916: a lowercase `function` referenced as a method
        // value (`this.x = fn`) receives the instance as `this` at call time.
        let src = "function shouldUpdate(nextProps) {\n  const ref = this.props.ref;\n  return shallowDiffers(this.props, nextProps);\n}\nfunction Memoed(props) {\n  this.shouldComponentUpdate = shouldUpdate;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_invoked_with_call_this() {
        // Regression for #1916: a lowercase `function` invoked elsewhere via
        // `.call(this)` is explicitly bound, so `this` in its body is valid.
        let src = "function init() {\n  return this.x;\n}\nfunction Widget() {\n  init.call(this);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_lowercase_free_function() {
        // Negative: an ordinary lowercase free function never used as a
        // constructor or bound method still has a stray `this`.
        let diags = run_on("function foo() {\n  return this.bar;\n}\nfoo();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_function_declaration_referenced_as_shorthand_property() {
        // Regression for #6825: fastify declares its public-API methods as
        // standalone `function` declarations and collects them into the instance
        // object with shorthand properties (`const fastify = { inject, addHook }`).
        // Called as `fastify.inject(...)`, the object is the receiver.
        let src = "function inject (opts) {\n  return this.ready(opts);\n}\nfunction addHook (name, fn) {\n  this.after(fn);\n  return this;\n}\nconst fastify = {\n  version: '1.0.0',\n  inject,\n  addHook\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_declaration_referenced_as_explicit_key_property() {
        // The explicit `{ key: fn }` spelling of the shorthand form above lands
        // the function in the same method slot, so it binds `this` the same way.
        let src = "function inject (opts) {\n  return this.ready(opts);\n}\nconst fastify = { inject: inject };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_collected_into_an_array_not_an_object() {
        // Negative-space guard for #6825: an array element is not a property
        // slot, so no call site supplies a receiver and the `this` stays unbound.
        let src = "function inject (opts) {\n  return this.ready(opts);\n}\nconst all = [inject];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_this_in_function_referenced_only_as_a_computed_property_key() {
        // Negative-space guard for #6825: a reference in computed-key position is
        // not the property value, so it installs nothing and supplies no receiver.
        let src = "function inject () {\n  return this.ready();\n}\nconst byFn = { [inject]: 1 };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_this_in_mixed_use_function_with_one_receiver_supplying_reference() {
        // Pins the aggregation for #8191: one receiver-supplying reference
        // exempts the body, even when another reference calls the function with
        // no receiver. prettier's Flow fixture `object-method/test3.js` is the
        // shape — `bar(foo)` reaches an unbound call, `qux({ f: foo })` a bound
        // one — and the rule reads the slot, never the call behind it.
        let src = "function foo() {\n  this.m();\n}\nfunction bar(f) {\n  f();\n}\nbar(foo);\nfunction qux(o) {\n  o.f();\n}\nqux({ f: foo });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_mixed_use_function_assigned_as_method_and_called_bare() {
        // Pins the aggregation for #8191 on the method-value branch: a bare call
        // beside `o.m = foo` does not withdraw the exemption.
        let src = "function foo() {\n  this.m();\n}\nfoo();\nconst o = {};\no.m = foo;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_mixed_use_var_bound_function() {
        // Pins the aggregation for #8191 on the var-held function-expression
        // branch, which folds its reference set the same way.
        let src = "const f = function () {\n  this.m();\n};\nf();\nconst o = {};\no.m = f;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_underscore_prefixed_constructor_function() {
        // Regression for #3357: a module-private constructor function follows the
        // `_PascalCase` convention — after stripping the leading underscore the
        // initial is uppercase, so it is a constructor and the `this.*` instance
        // setup is valid. fastify's `lib/reply.js` builds `_Reply` this way and
        // wires its prototype chain with `Object.setPrototypeOf`.
        let src = "function buildReply (R) {\n  function _Reply (res, request, log) {\n    this.raw = res\n    this.request = request\n    this[kReplyHeaders] = {}\n  }\n  Object.setPrototypeOf(_Reply.prototype, R.prototype)\n  Object.setPrototypeOf(_Reply, R)\n  return _Reply\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_underscore_prefixed_lowercase_function() {
        // Negative-space guard for #3357: stripping leading underscores must not
        // turn a lowercase-initial function into a constructor — `_reply` is not
        // PascalCase, so its stray `this` must still fire.
        let diags = run_on("function _reply() {\n  return this.x;\n}\n_reply();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_function_with_jsdoc_type_alias() {
        // Regression for #1775: a `.js` function whose JSDoc `@type` assigns an
        // aliased function type that declares `this` (`type Equals = (this:
        // Value, …) => boolean`) is type-checked against that contract.
        let src = "/** @type {Equals} */\nexport function equals(value) {\n  return value === this.v;\n}\n\n/** @type {Equals} */\nexport function safe_equals(value) {\n  return !safe_not_equal(value, this.v);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_with_inline_jsdoc_this_type() {
        // Regression for #1775: an inline `@type {(this: T, …) => …}` declares
        // the `this` binding directly in the function signature.
        let src = "/** @type {(this: Value, value: unknown) => boolean} */\nexport function equals(value) {\n  return value === this.v;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_with_jsdoc_this_tag() {
        // Regression for #1775: the `@this {T}` tag names the `this` context.
        let src = "/** @this {Value} */\nexport function equals(value) {\n  return value === this.v;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_with_unrelated_jsdoc() {
        // Negative: a JSDoc block without `@type`/`@this` does not declare a
        // `this` context, so a stray `this` must still fire.
        let diags = run_on("/** Does a thing. @param value - input */\nexport function equals(value) {\n  return value === this.v;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_chai_add_method_callback() {
        // Regression for #1549: Chai binds the Assertion instance to `this`
        // inside a `function` passed to `chai.Assertion.addMethod(...)`.
        let src = "chai.Assertion.addMethod('x', function () {\n  return this._obj;\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_chai_overwrite_property_callback() {
        // Regression for #1549: the other Chai plugin-registration methods
        // (`addProperty`/`overwriteMethod`/`overwriteProperty`) and a bare
        // `Assertion` receiver bind `this` the same way.
        let src = "Assertion.overwriteProperty('ok', function () {\n  return this.assert(this._obj);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_free_function_at_module_scope() {
        // Negative-space guard for #1549: a free `function` at module scope is
        // not a Chai registration callback — `this` is unbound and must fire.
        let diags = run_on("function f() {\n  return this.x;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_array_foreach_callback() {
        // Negative-space guard for #1549: a bare `function` passed to `forEach`
        // is a genuine invalid-this — the Chai allowance must not leak to it.
        let diags = run_on("[1].forEach(function () {\n  return this.x;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_non_assertion_add_method_callback() {
        // Negative-space guard for #1549: `addMethod` on a non-Assertion
        // receiver is not the Chai API — `this` stays unbound and must fire.
        let diags = run_on("registry.addMethod('x', function () {\n  return this._obj;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_chai_add_chainable_method_callback_by_reference() {
        // Regression for #6445: chai's `lib/chai/core/assertions.js` declares
        // named functions that use `this` and registers them by identifier
        // (`function an(...) { this.assert(...) }` then
        // `Assertion.addChainableMethod('an', an)`). Chai invokes the function
        // with `this` bound to the Assertion instance, so the body's `this` is
        // valid even though the function node is not itself a call argument.
        let src = "function an(type, msg) {\n  if (msg) flag(this, 'message', msg);\n  this.assert(type === detectedType, 'expected #{this} to be a ' + type);\n}\nAssertion.addChainableMethod('an', an);\nAssertion.addChainableMethod('a', an);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_chai_add_chainable_method_inline_callback() {
        // Regression for #6445: the chainable-method names also exempt the inline
        // form (`Assertion.addChainableMethod('x', function () {…})`), since both
        // the direct-argument and by-reference paths read the same method set.
        let src = "Assertion.addChainableMethod('x', function () {\n  return this._obj;\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_chai_overwrite_chainable_method_callback_by_reference() {
        // Regression for #6445: the `overwriteChainableMethod` registration and
        // the `chai.Assertion` member receiver bind `this` the same way when the
        // callback is passed by identifier reference.
        let src = "function lengthOf() {\n  return this._obj.length;\n}\nchai.Assertion.overwriteChainableMethod('length', lengthOf, chainer);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_named_function_never_registered_with_chai() {
        // Negative-space guard for #6445: a named function that uses `this` but is
        // never passed to a Chai registration method has no bound `this` — must
        // still fire. `an` is referenced only by an ordinary call here.
        let src = "function an(type) {\n  return this.assert(type);\n}\nan('number');";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_named_function_registered_with_non_chai_method() {
        // Negative-space guard for #6445: passing the function's name to a
        // non-Chai registration call (a `registry.addChainableMethod` on a
        // non-Assertion receiver) does not bind `this` — must still fire.
        let src = "function an(type) {\n  return this.assert(type);\n}\nregistry.addChainableMethod('an', an);";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_promise_then_callback() {
        // Negative: a `function` callback passed to a plain Promise `.then()`
        // (no `cy` chain root) gets no bound `this` — must still fire.
        let diags = run_on("fetch('/x').then(function () {\n  return this.value;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_event_emitter_on_callback() {
        // Regression for #3884: Node binds `this` to the emitter inside a
        // non-arrow `function` listener callback (`body.on('data', function () {
        // this.used })`). undici registers listeners this way throughout `lib`.
        let src = "body.on('data', function () {\n  this.used = true;\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_event_emitter_prototype_call_callback() {
        // Regression for #3884: the `EE.prototype.on.call(body, …)` reflection
        // form registers the listener on `body`, so Node still binds `this` to
        // the emitter inside the callback (`lib/core/util.js`).
        let src = "EventEmitter.prototype.on.call(body, 'end', function () {\n  this.done = true;\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_event_emitter_listener_method_variants() {
        // Regression for #3884: every EventEmitter listener-registration method
        // (`once`/`addListener`/`prependListener`/`prependOnceListener`) binds
        // `this` to the emitter the same way `on` does.
        for method in ["once", "addListener", "prependListener", "prependOnceListener"] {
            let src = format!("emitter.{method}('e', function () {{\n  this.x = 1;\n}});");
            assert!(run_on(&src).is_empty(), "method `{method}` should be exempt");
        }
    }

    #[test]
    fn allows_this_in_private_field_emitter_on_callback() {
        // Regression for #3884: the receiver can be any expression, including a
        // private-field member (`this.#writeStream.on('close', function () {…})`
        // from `lib/handler/cache-handler.js`).
        let src = "class H {\n  #s;\n  m() {\n    this.#s.on('close', function () {\n      this.closed = true;\n    });\n  }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_as_first_arg_of_on_call() {
        // Negative-space guard for #3884: the exemption applies only to the
        // callback *argument*. A `function` in the callee position of an `on`
        // call (e.g. an IIFE) is not a listener callback — `this` stays unbound.
        let diags = run_on("(function () {\n  return this.x;\n})();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_non_listener_method_callback() {
        // Negative-space guard for #3884: a method name outside the listener set
        // (`addEventListener` is DOM, not EventEmitter; `subscribe` is unrelated)
        // does not bind `this` to a receiver — must still fire.
        let diags = run_on("source.subscribe(function () {\n  return this.x;\n});");
        assert_eq!(diags.len(), 1);
    }

    /// The declarations of #8296's repro: a callable contract carrying a `this`
    /// parameter, the hook that takes it, and an emitter whose listener parameter
    /// declares none.
    const HOOK_DECLARATIONS: &str = "type Ctx = { timeout: (ms: number) => void }\ntype Func = (this: Ctx) => void\ndeclare function before(fn: Func): void\ndeclare function it(title: string, fn: Func): void\ndeclare const emitter: { on: (e: string, fn: () => void) => void }\n";

    #[test]
    fn allows_this_in_callback_whose_parameter_type_declares_this() {
        // Regression for #8296: `before(function () { this.timeout(1000) })` is
        // checked against `before`'s parameter type — `(this: Ctx) => void` —
        // which supplies the `this` binding, exactly as the same contract does
        // when written on a variable (`const hook: Func = function () {…}`).
        // The async form is the shape all 41 kysely diagnostics take.
        let src = format!(
            "{HOOK_DECLARATIONS}before(async function () {{\n  this.timeout(1000)\n}})\nbefore(function () {{\n  this.timeout(1000)\n}})\nit('does a thing', function () {{\n  this.timeout(1000)\n}})"
        );
        assert!(run_on(&src).is_empty());
    }

    #[test]
    fn flags_this_in_callback_whose_parameter_type_declares_no_this() {
        // Regression for #8296: the resolved contract is authoritative in both
        // directions. `emitter.on`'s parameter is `() => void`, which declares no
        // `this`, so the callback really is unbound (TS2683) — the listener-method
        // name must not exempt it when the signature says otherwise.
        let src = format!(
            "{HOOK_DECLARATIONS}emitter.on('data', function () {{\n  this.timeout(1000)\n}})"
        );
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn flags_this_in_standalone_function_beside_typed_hooks() {
        // Negative-space guard for #8296: a `function` that is never handed to a
        // callee has no contract to read — its `this` stays unbound and fires.
        let src = format!("{HOOK_DECLARATIONS}export function loose(): void {{\n  this.timeout(1000)\n}}");
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn flags_this_in_callback_of_locally_declared_hook_without_this_contract() {
        // Negative-space guard for #8296: a `before` declared in this file with a
        // `this`-less callback type is not the test runner's — its own signature
        // answers, and it declares no receiver.
        let src = "declare function before(fn: () => void): void\nbefore(function () {\n  return this.x\n})";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_this_in_ambient_mocha_hook_callback() {
        // Regression for #8296: in a mocha suite the hooks are ambient globals
        // from `@types/mocha`, so no signature is reachable from the file. The
        // runner's published contract answers: it invokes the callback with
        // `this` bound to the Context (`test/node/src/replace.test.ts` in kysely).
        let src = "describe('replace into', () => {\n  before(async function () {\n    ctx = await initTest(this, dialect)\n  })\n  it('works', function () {\n    this.timeout(1000)\n  })\n})";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_ambient_hook_member_and_template_forms() {
        // Regression for #8296: `it.only`, `it.skip` and `describe.each(table)(…)`
        // register through the same hook, so they carry the same contract as the
        // bare call — the callee is read at its root identifier.
        for callee in ["it.only", "it.skip", "test.each(table)"] {
            let src = format!("{callee}('x', function () {{\n  this.timeout(1000)\n}})");
            assert!(run_on(&src).is_empty(), "`{callee}` should be exempt");
        }
    }

    #[test]
    fn allows_this_in_imported_hook_callback() {
        // Regression for #8296: vitest/jest suites import their hooks, so the
        // signature still lives outside the file and the runner's contract answers.
        let src = "import { beforeEach } from 'vitest'\nbeforeEach(function () {\n  this.timeout(1000)\n})";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_arrow_passed_to_a_test_hook() {
        // Negative-space guard for #8296: an arrow has no `this` of its own, so no
        // callee contract can bind it — `this` inside it reads the enclosing
        // scope's and must still fire.
        let diags = run_on("before(() => {\n  this.timeout(1000)\n})");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_callback_of_non_hook_callee() {
        // Negative-space guard for #8296: an ordinary callee outside the runner
        // and library contracts supplies no receiver — `this` stays unbound.
        let diags = run_on("schedule('x', function () {\n  return this.timeout;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_function_with_explicit_this_param_returning_this() {
        // Regression for #1342: a fluent function declaring an explicit
        // TypeScript `this` parameter (`function use(this: unknown, …)`) types
        // its `this` context, so returning `this` from the body is valid.
        let src = "function use(this: unknown, url: string | null) {\n  return this;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_typed_constructor_function_with_this_param() {
        // Regression for #1342: an old-style constructor function with an
        // explicit `this` parameter (`function Holder(this: HolderInstance)`)
        // declares the type of `this`, so assigning `this.*` is valid.
        let src = "function Holder(this: HolderInstance) {\n  this.req = null;\n  this.res = null;\n  this.url = null;\n  this.context = null;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_typed_via_matcher_function_alias() {
        // Regression for #2120: a `function` expression assigned to a variable
        // typed with a function-type alias (`MatcherFunction<…>`, whose signature
        // carries a `this: MatcherContext`) is typed against a callable contract
        // that supplies `this` — the official Jest custom-matcher pattern.
        let src = "const toBeWithinRange: MatcherFunction<[floor: unknown, ceiling: unknown]> = function (actual, floor, ceiling) {\n  return { pass: this.equals(actual, floor), message: () => '' };\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_typed_via_inline_function_type() {
        // Regression for #2120: an inline function-type annotation on the binding
        // (`const m: (this: T, …) => …`) declares the `this` binding directly.
        let src = "const equals: (this: Value, value: unknown) => boolean = function (value) {\n  return value === this.v;\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_with_non_callable_binding_annotation() {
        // Negative-space guard for #2120: the typed-binding exemption only covers
        // function-type annotations. A function nested inside a non-callable typed
        // binding's initializer still has an unbound `this` and must fire.
        let diags = run_on("const x: number[] = [1].map(function () {\n  return this.v;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_function_with_untyped_binding() {
        // Negative-space guard for #2120: a `function` assigned to a binding with
        // no type annotation has no callable contract — `this` is unbound and
        // must fire.
        let diags = run_on("const f = function () {\n  return this.v;\n};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_array_map_callback_with_trailing_this_arg() {
        // Regression for #3812: `Array.prototype.map(callbackFn, thisArg)` binds
        // `this` inside the non-arrow callback to the trailing `thisArg`, so
        // `this` in the callback body is the bound context, not unbound.
        let src = "class Foo {\n  vals = [];\n  run() {\n    return [1, 2, 3].map(function (x) {\n      return x + this.vals.length;\n    }, this);\n  }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_util_callback_with_trailing_this_arg() {
        // Regression for #3812: the `(collection, callback, context)` util-library
        // convention (zrender `map`/`each`, lodash) passes the `thisArg` after the
        // callback — `this` in the callback is the bound context. The trailing
        // `this` argument sits in a class method so it is itself a valid context.
        let src = "class Foo {\n  run() {\n    return map(arr, function () {\n      return this.x;\n    }, this);\n  }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_foreach_callback_with_local_var_this_arg() {
        // Regression for #5169: the trailing `thisArg` need not be the literal
        // `this` keyword — `Array.prototype.forEach(callbackFn, thisArg)` binds
        // `this` inside the non-arrow callback to whatever value (here a local
        // variable) is passed after the callback.
        let src = "const ctx = { x: 1 };\n[1, 2, 3].forEach(function () {\n  return this.x;\n}, ctx);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_rxjs_predicate_callback_with_local_var_this_arg() {
        // Regression for #5169: RxJS predicate operators (`every`/`filter`/`find`/
        // `map`) accept a trailing `thisArg` that binds `this` inside the non-arrow
        // callback, exactly like the Array methods. The `thisArg` here is a local
        // variable, not the literal `this`.
        let src = "const thisArg = { limit: 5 };\nof(1, 2, 3).pipe(every(function (val) {\n  const limit = this.limit;\n  return val < limit;\n}, thisArg));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_map_callback_without_trailing_this_arg() {
        // Negative-space guard for #3812: a callback with no trailing `thisArg`
        // gets no bound `this` — must still fire.
        let diags = run_on("[1, 2, 3].map(function () {\n  return this.x;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_callback_with_nested_this_arg_before_it() {
        // Negative-space guard for #7172: only a *direct* `this` argument before
        // the callback binds its receiver. A `this` nested in a sub-expression
        // (`foo(bar(this), fn)`) is data, not a receiver, so the callback's `this`
        // stays unbound and must fire. The nested `this` sits in a class method so
        // only the callback's `this` is flagged.
        let src = "class Foo {\n  run() {\n    return foo(bar(this), function () {\n      return this.x;\n    });\n  }\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_generator_bound_via_leading_this_arg() {
        // Regression for #7172: `STM.gen(this, function* () {…})` binds the
        // generator's `this` to its leading argument (`body.call(self)`), so
        // `this` inside the generator is the enclosing receiver and is valid.
        let src = "class TSubscriptionRefImpl {\n  peek = STM.gen(this, function* () {\n    const x = yield* this.peekOption;\n    return x;\n  });\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_effect_gen_bound_via_leading_this_arg() {
        // Regression for #7172: `Effect.gen(this, function* () {…})` is the same
        // leading-receiver generator-adapter idiom as `STM.gen`.
        let src = "class C {\n  run = Effect.gen(this, function* () {\n    if (this.dependencies) {\n      return 1;\n    }\n    return 0;\n  });\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_effect_gen_nested_in_outer_call() {
        // Regression for #7172: the receiver-binding call can itself be nested in
        // an outer call (`Layer.scopedContext(Effect.gen(this, function* () {…}))`);
        // the callback's immediately enclosing call still supplies the leading
        // `this`, so `this` in the generator is valid.
        let src = "class C {\n  layer = Layer.scopedContext(Effect.gen(this, function* () {\n    return this.x;\n  }));\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_settimeout_callback_with_primitive_delay() {
        // Negative-space guard for #7172: a callback whose only sibling argument is
        // a primitive literal (`setTimeout(function () {…}, 100)`'s delay) gets no
        // receiver — the trailing `100` cannot serve as `this`, so `this` in the
        // callback stays unbound and must fire.
        let diags = run_on("setTimeout(function () {\n  return this.x;\n}, 100);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_var_bound_function_called_via_bind() {
        // Regression for #4985: dayjs's localeData plugin holds an anonymous
        // `function` in a `const` and invokes it via `localeData.bind(this)()` at
        // its only call site, so `this` is supplied at the binding site. `proto`
        // aliases the Dayjs prototype, so `proto.localeData = function () {}` is a
        // prototype method whose own `this` is also bound.
        let src = "const proto = Dayjs.prototype;\nconst localeData = function () {\n  return {\n    firstDayOfWeek: () => this.$locale().weekStart || 0,\n    meridiem: this.$locale().meridiem,\n  };\n};\nproto.localeData = function () {\n  return localeData.bind(this)();\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_var_bound_function_called_via_call() {
        // Regression for #4985: the `.call(this)` / `.apply(this)` binding forms
        // on a var-held function expression supply `this` the same way `.bind`
        // does.
        let src = "const fn = function () {\n  return this.x;\n};\nfn.call(obj);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_var_bound_function_placed_in_object_literal() {
        // A `function` expression held in a `const` and then collected into an
        // object literal reaches the same method slot as a `function` declaration
        // does, so the object supplies its receiver.
        let src = "const inject = function () {\n  return this.ready();\n};\nconst fastify = { inject };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_var_bound_function_never_bound() {
        // Negative-space guard for #4985: a `function` held in a `const` but never
        // referenced via `.bind`/`.call`/`.apply`/`new`/method-value has no bound
        // `this` and must still fire (this is the existing untyped-binding case).
        let diags = run_on("const fn = function () {\n  return this.x;\n};\nfn();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_plain_standalone_mixin_function() {
        // Negative-space guard for #4985: i18next's `formatLanguageCode` is a
        // plain standalone function with no explicit `this` parameter and no
        // detectable binding (it is mixed onto the instance at runtime). The rule
        // cannot distinguish it from a real bug, so it stays flagged — the fix is
        // to add an explicit `this:` parameter.
        let src = "export function formatLanguageCode(code) {\n  if (this.options.lowerCaseLng) {\n    return code.toLowerCase();\n  }\n  return code;\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_jquery_map_callback() {
        // Regression for #5192: jQuery/cheerio bind `this` to the current element
        // inside a non-arrow `function` iterator callback. Wrapping it as
        // `$(this)` is the documented idiom (mjml's `wrapper-gap.test.js`), so
        // every `this` in the callback body is the bound element, not unbound.
        let src = "$('.my-section')\n  .map(function getAttr() {\n    const str = $(this).attr('style');\n    if (str.includes('margin-top:')) {\n      return $(this).attr('style');\n    }\n    return undefined;\n  })\n  .get();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_jquery_each_callback() {
        // Regression for #5192: `.each(function () { $(this).hide() })` is the
        // same caller-binds-`this` idiom as `.map`.
        let src = "$(sel).each(function () {\n  $(this).hide();\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_callback_without_jquery_wrap() {
        // Negative-space guard for #5192: a non-arrow `function` callback that
        // uses `this.x` directly with no `$(this)` wrap has no caller-binding
        // evidence — `this` stays unbound and must fire.
        let diags = run_on("list.map(function () {\n  return this.x;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_at_top_level_even_with_jquery_in_scope() {
        // Negative-space guard for #5192: a `this` at module scope (no enclosing
        // non-arrow `function`) is never reached by the `$(this)` body scan and
        // stays flagged.
        let diags = run_on("$(this);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_arrow_callback_with_jquery_wrap() {
        // Negative-space guard for #5192: an arrow function cannot have its `this`
        // rebound by the caller — `$(this)` inside an arrow at module scope reads
        // the module `this`, a genuine bug, so it must still fire.
        let diags = run_on("list.map(() => {\n  return $(this).text();\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_outer_function_this_when_only_nested_function_uses_jquery() {
        // Negative-space guard for #5192: the `$(this)` exemption binds the
        // function that directly contains it. An outer standalone `function` with
        // a stray `this.x` is not rescued by a *nested* inner callback's legit
        // `$(this)` — the outer `this` must still fire, only the inner is exempt.
        let src = "function outer() {\n  const v = this.x;\n  list.each(function () {\n    return $(this).text();\n  });\n  return v;\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_function_without_explicit_this_param() {
        // Negative-space guard for #1342: a plain standalone function with no
        // explicit `this` parameter (and outside any class/object method) still
        // has an unbound `this` and must fire.
        let diags = run_on("function f(url: string) {\n  return this.x;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_as_reflect_apply_this_arg_in_wrapper_function() {
        // Regression for #6584: a debounce wrapper forwards its caller's receiver
        // to the wrapped function via `Reflect.apply(fn, this, arguments)` — the
        // `this` is the second (`thisArg`) argument being forwarded, the standard
        // idiom equivalent to `fn.apply(this, args)`. The standalone `function`
        // returned by `debounce` is cast `as T` where `T extends (this: unknown,
        // …) => void`, so `this` is intentional, not a stray reference.
        let src = "export const debounce = <T extends (this: unknown, ...args: any[]) => void>(\n  originalFunction: T,\n  duration: number,\n): T => {\n  let timeout: NodeJS.Timeout | undefined;\n  return function () {\n    if (timeout) {\n      clearTimeout(timeout);\n    }\n    timeout = setTimeout(\n      () => Reflect.apply(originalFunction, this, arguments),\n      duration,\n    );\n  } as T;\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_as_apply_this_arg_in_metadata_wrapper() {
        // Regression for #8099: libphonenumber-js's metadata-argument wrapper
        // forwards its caller's receiver with the plain `fn.apply(this, args)`
        // spelling — the same receiver-forwarding idiom as `Reflect.apply(fn,
        // this, args)`, written the way every pre-`Reflect` wrapper writes it.
        let src = "export function withDefaults(func, args) {\n  return func.apply(this, args);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_as_call_this_arg_in_wrapper() {
        // Regression for #8099: `fn.call(this, …)` puts the forwarded receiver in
        // the same first-argument `thisArg` slot as `fn.apply(this, args)`.
        let src = "export function withCall(func, a) {\n  return func.call(this, a);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_as_non_first_arg_of_apply() {
        // Negative-space guard for #8099: only the *first* argument of
        // `<callee>.call`/`.apply` is the `thisArg`. A `this` passed as ordinary
        // data (`fn.apply(ctx, this)`) forwards no receiver and must still fire.
        let diags = run_on("function f() {\n  return fn.apply(ctx, this);\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_standalone_function_with_unrelated_this_use() {
        // Negative-space guard for #6584: the `Reflect.apply` exemption keys on
        // the `this` node being the second argument of a `Reflect.apply` call. A
        // module-scope standalone function whose `this` is used elsewhere is not
        // that idiom — `this` stays unbound and must fire.
        let diags = run_on("function f() {\n  return this.x;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_as_first_arg_of_reflect_apply() {
        // Negative-space guard for #6584: `this` as the *first* argument of
        // `Reflect.apply` (the function to invoke, wrong position) is not the
        // forwarding idiom — keep current behavior and flag it.
        let diags = run_on("function f() {\n  return Reflect.apply(this, ctx, args);\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_as_second_arg_of_non_reflect_apply_call() {
        // Negative-space guard for #6584: passing `this` as the second argument of
        // some other call (not `Reflect.apply`) does not forward a receiver — the
        // exemption keys on the `Reflect.apply` callee shape, so `this` still fires.
        let diags = run_on("function f() {\n  return helper(fn, this, args);\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_buried_in_second_arg_of_reflect_apply() {
        // Negative-space guard for #6584: only `this` written *directly* as the
        // second argument is the forwarding idiom. A `this` buried in a
        // sub-expression of arg 1 (`Reflect.apply(fn, this.ctx, args)`) has the
        // member access as its immediate parent, not the `Reflect.apply` call, so
        // it stays unbound and must fire.
        let diags = run_on("function f() {\n  return Reflect.apply(fn, this.ctx, args);\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_function_returned_from_callable_alias_return_type() {
        // Regression for #7213: a `function` returned from a function whose
        // explicit return type is a callable type alias (`function
        // createLoadHandler(): LoadHandler { return async function (id) {…} }`,
        // `type LoadHandler = (this: Ctx, id: string) => void`) is type-checked
        // against that alias's `this: Ctx` binding — the idiomatic Rollup/esbuild
        // plugin-hook shape — so `this` in the returned body is valid.
        let src = "type LoadHandler = (this: Ctx, id: string) => void;\nfunction createLoadHandler(): LoadHandler {\n  return async function (id) {\n    this.addWatchFile(id);\n  };\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_returned_from_inline_function_return_type() {
        // Regression for #7213: the inline function-type return annotation
        // (`function f(): (this: T) => void`) declares the `this` binding
        // directly, exactly like the aliased form.
        let src = "function f(): (this: T) => void {\n  return function () {\n    return this.x;\n  };\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_returned_from_untyped_function() {
        // Negative-space guard for #7213: a `function` returned from a function
        // with no return-type annotation has no callable contract supplying
        // `this` — `this` in the returned body is unbound and must fire.
        let diags = run_on("function bad() {\n  return function () {\n    return this.x;\n  };\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_own_body_of_function_with_return_type() {
        // Negative-space guard for #7213: the exemption keys on a `function` in
        // *return position*, not on the enclosing function itself. A `this`
        // written directly in the body of a function that merely carries a
        // return-type annotation — not inside a returned function — is still
        // unbound and must fire.
        let diags = run_on("function f(): void {\n  return this.x;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_captured_into_local_forwarded_via_apply() {
        // Regression for #7403: the `var self = this` context-capture idiom. The
        // debounce factory returns a non-arrow `function` so `this` binds to the
        // call site; `this` is captured into `const context` and forwarded as the
        // `thisArg` of `fn.apply(context, args)` inside the nested `later` closure,
        // so the `this` reference is intentional, not a stray bug.
        let src = "export function debounce(fn) {\n  let waiting;\n  return function() {\n    if (waiting) return;\n    waiting = true;\n    const context = this,\n      args = arguments;\n    const later = function() {\n      waiting = false;\n      fn.apply(context, args);\n    };\n    nextTick(later);\n  };\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_captured_into_local_forwarded_via_call() {
        // Regression for #7403: `this` captured into a local and forwarded as the
        // first (thisArg) argument of `<callee>.call(X, …)` is the same idiom.
        let src = "function f() {\n  const self = this;\n  other.call(self);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_captured_into_local_forwarded_via_reflect_apply() {
        // Regression for #7403: forwarding the captured local as the second
        // (thisArg) argument of `Reflect.apply(fn, X, args)` is recognized too,
        // reusing the same direct-thisArg check the literal-`this` path uses.
        let src = "function g() {\n  const that = this;\n  Reflect.apply(fn, that, []);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_captured_into_local_never_forwarded_as_this_arg() {
        // Negative-space guard for #7403: the exemption keys on the captured local
        // being *forwarded as a thisArg*. A `this` captured into a local that is
        // only read elsewhere (never a `.call`/`.apply`/`Reflect.apply` thisArg)
        // has no receiver-binding evidence — `this` stays unbound and must fire.
        let diags = run_on("function h() {\n  const x = this;\n  console.log(x);\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_captured_into_local_used_as_reflect_apply_function_arg() {
        // Negative-space guard for #7403: `Reflect.apply`'s first argument is the
        // function to invoke, not the receiver. A local capturing `this` passed
        // there (`Reflect.apply(x, ctx, args)`) is not a thisArg forward — keep the
        // current behavior and flag it, mirroring the direct-`this` first-arg case.
        let diags = run_on("function f() {\n  const x = this;\n  Reflect.apply(x, ctx, args);\n}");
        assert_eq!(diags.len(), 1);
    }
}

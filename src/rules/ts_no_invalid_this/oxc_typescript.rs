//! ts-no-invalid-this OXC backend — flag `this` expressions outside
//! classes/object methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use oxc_ast::CommentKind;
use oxc_ast::ast::{AssignmentTarget, BindingPattern, Expression, TSType};
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
    declarator.type_annotation.as_ref().is_some_and(|ann| {
        matches!(
            ann.type_annotation,
            TSType::TSTypeReference(_) | TSType::TSFunctionType(_)
        )
    })
}

/// Mocha test/suite globals whose `function` callback is invoked with a
/// Test/Suite context bound to `this` (`this.timeout`, `this.retries`,
/// `this.skip`, `this.slow`).
const MOCHA_GLOBALS: &[&str] = &[
    "describe", "it", "before", "after", "beforeEach", "afterEach", "context",
    "specify",
];

/// True when `callee` is a Mocha global, either bare (`it(...)`) or a
/// `.only`/`.skip` variant (`describe.only(...)`, `it.skip(...)`).
fn callee_is_mocha_global(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(ident) => {
            MOCHA_GLOBALS.contains(&ident.name.as_str())
        }
        Expression::StaticMemberExpression(member)
            if matches!(member.property.name.as_str(), "only" | "skip") =>
        {
            matches!(
                &member.object,
                Expression::Identifier(ident)
                    if MOCHA_GLOBALS.contains(&ident.name.as_str())
            )
        }
        _ => false,
    }
}

/// True when `func_id` is a `function` expression passed directly as an argument
/// to a Mocha global (`describe`/`it`/`before`/... and `.only`/`.skip`
/// variants). Mocha binds a Test/Suite context to `this` in such callbacks, so
/// `this` inside the function body is valid. The function expression is the
/// parent or grandparent's child of the `CallExpression` depending on whether
/// the AST wraps arguments.
fn is_mocha_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_id);
    let call = match nodes.kind(parent_id) {
        AstKind::CallExpression(call) => call,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            let AstKind::CallExpression(call) = nodes.kind(gp_id) else {
                return false;
            };
            call
        }
    };
    callee_is_mocha_global(&call.callee)
}

/// True when `expr` is a Cypress command chain rooted in the `cy` global
/// (`cy`, `cy.get(...)`, `cy.get(...).as(...).contains(...)`, …). The chain is a
/// left-spine of member accesses and calls that bottoms out at the `cy`
/// identifier.
fn is_cypress_chain(expr: &Expression) -> bool {
    let mut current = expr;
    loop {
        match current {
            Expression::Identifier(ident) => return ident.name == "cy",
            Expression::CallExpression(call) => current = &call.callee,
            Expression::StaticMemberExpression(member) => current = &member.object,
            Expression::ComputedMemberExpression(member) => current = &member.object,
            _ => return false,
        }
    }
}

/// True when `func_id` is a `function` expression passed as an argument to a
/// `.then(...)`/`.should(...)` member call on a Cypress chain
/// (`cy.get(...).then(function () { this.alias })`). Cypress binds the shared
/// test context to `this` in such callbacks — aliases registered via
/// `.as('name')` are read as `this.name` — so `this` inside the body is valid.
fn is_cypress_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_id);
    let call = match nodes.kind(parent_id) {
        AstKind::CallExpression(call) => call,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            let AstKind::CallExpression(call) = nodes.kind(gp_id) else {
                return false;
            };
            call
        }
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "then" | "should") {
        return false;
    }
    is_cypress_chain(&member.object)
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
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_id);
    let call = match nodes.kind(parent_id) {
        AstKind::CallExpression(call) => call,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            let AstKind::CallExpression(call) = nodes.kind(gp_id) else {
                return false;
            };
            call
        }
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
/// `is_constructor_function` uses to trace how a named function is later used.
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
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_id);
    let call = match nodes.kind(parent_id) {
        AstKind::CallExpression(call) => call,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            let AstKind::CallExpression(call) = nodes.kind(gp_id) else {
                return false;
            };
            call
        }
    };
    let func_span = nodes.kind(func_id).span();
    if !call.arguments.iter().any(|arg| arg.span() == func_span) {
        return false;
    }
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

/// True when `func_id` is a non-arrow `function` passed as a callback argument
/// to a `CallExpression` that also passes at least one trailing argument
/// (`arr.map(function () {…}, this)`, `arr.forEach(function () {…}, thisArg)`,
/// `of(…).pipe(every(function () {…}, thisArg))`). `Array.prototype.{map,forEach,
/// filter,…}`, RxJS predicate operators, and util libraries following the
/// `(collection, callback, context)` convention (zrender `map`/`each`, lodash)
/// invoke the callback with the trailing argument bound as `this`, so `this` in
/// the callback body is the bound context. The trailing argument is the `thisArg`
/// whether it is the literal `this` keyword or any other value (a local variable,
/// an object literal, …).
///
/// The `thisArg` must come *after* the callback in the argument list — an argument
/// passed before the callback (`foo(this, function () {…})`) is data, not the
/// `thisArg`, so it does not bind the callback's `this`.
fn is_callback_with_trailing_this_arg(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_id);
    let call = match nodes.kind(parent_id) {
        AstKind::CallExpression(call) => call,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            let AstKind::CallExpression(call) = nodes.kind(gp_id) else {
                return false;
            };
            call
        }
    };
    let func_span = nodes.kind(func_id).span();
    let Some(callback_index) = call
        .arguments
        .iter()
        .position(|arg| arg.span() == func_span)
    else {
        return false;
    };
    callback_index + 1 < call.arguments.len()
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
/// - `x.member = F` — assigned as a method value (receives the receiver as `this`).
fn reference_binds_this(
    ref_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
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
            ) && assign.right.span() == nodes.kind(ref_node_id).span()
        }
        _ => false,
    }
}

/// True when the standalone `function` at `func` is a constructor function whose
/// `this` is bound at call time — either by the PascalCase naming convention, or
/// because its name is referenced as `new`/`.call`/`.apply`/`.bind`/method-value
/// somewhere in the module.
fn is_constructor_function(
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
/// `name.apply(...)`, `new name(...)`, or assigned as a method value
/// (`x.member = name`). This generalizes the named-function logic in
/// `is_constructor_function` to anonymous function expressions held in a variable:
/// `const localeData = function () { … this.$locale() … }` that is later invoked
/// via `localeData.bind(this)()` (the dayjs plugin / bound-method idiom) has its
/// `this` supplied at the binding site, so `this` in the body is intentional.
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

fn is_valid_this_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    // `Reflect.apply(fn, this, args)`: the `this` is written directly as the
    // `thisArg` (second) argument, forwarding the enclosing function's receiver
    // to `fn` — the standard idiom equivalent to `fn.apply(this, args)`. This is
    // a property of the `this` node itself, independent of the enclosing
    // function, so it is checked before the boundary walk.
    if is_reflect_apply_this_arg(node.id(), semantic) {
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
                // Method-property assignment: a function assigned to a member
                // of any object (`obj.method = function () {…}`, `obj[k] = …`)
                // is a method — when invoked as `obj.method(...)`, `this` is
                // bound to the receiver at call time, so `this` is valid. This
                // subsumes the `*.prototype` and `module.exports` patching idioms.
                if is_method_property_assignment(ancestor.id(), semantic) {
                    return true;
                }
                // Mocha callback: a `function` passed to `describe`/`it`/hooks
                // is invoked with a Test/Suite context bound to `this`
                // (`this.timeout()`, `this.retries()`), so `this` is valid.
                if is_mocha_callback(ancestor.id(), semantic) {
                    return true;
                }
                // Cypress callback: a `function` passed to `.then()`/`.should()`
                // on a `cy` chain is invoked with the shared test context bound
                // to `this` (`this.alias` from a prior `.as('alias')`), so
                // `this` is valid.
                if is_cypress_callback(ancestor.id(), semantic) {
                    return true;
                }
                // Chai plugin registration: a `function` passed to a Chai
                // plugin-registration method (`addMethod`/`addProperty`/
                // `addChainableMethod`/`overwriteMethod`/`overwriteProperty`/
                // `overwriteChainableMethod`) on a `chai.Assertion` receiver is
                // invoked with `this` bound to the Assertion instance — the
                // documented plugin API — so `this` is valid. The inline form
                // passes the function directly; the by-reference form passes a
                // named function declaration by identifier
                // (`function an() {…}; Assertion.addChainableMethod('an', an)`).
                if is_chai_registration_callback(ancestor.id(), semantic) {
                    return true;
                }
                if is_chai_registration_callback_by_reference(func, semantic) {
                    return true;
                }
                // EventEmitter listener callback: a `function` passed to a
                // listener-registration call (`emitter.on('e', function () {…})`,
                // `.once`/`.addListener`/`.prependListener`/`.prependOnceListener`,
                // and the `EE.prototype.on.call(emitter, …)` form) is invoked by
                // Node with `this` bound to the emitter, so `this` is valid.
                if is_event_emitter_listener_callback(ancestor.id(), semantic) {
                    return true;
                }
                // Trailing-thisArg callback: a `function` passed to a call that
                // also passes a later argument (`arr.map(function () {…}, this)`,
                // `arr.forEach(function () {…}, thisArg)`) is invoked with that
                // argument bound as `this` — the ECMAScript `thisArg` convention
                // shared by `Array.prototype.{map,forEach,…}`, RxJS predicate
                // operators, and `(collection, callback, context)` util libraries.
                if is_callback_with_trailing_this_arg(ancestor.id(), semantic) {
                    return true;
                }
                // jQuery/cheerio iterator callback: a non-arrow `function` whose
                // body wraps `this` as `$(this)` (`.map(function () { $(this) })`,
                // `.each(...)`, …) has had its `this` rebound by the library to the
                // current element, so `this` in the body is the bound element.
                if function_body_has_jquery_this(ancestor.id(), semantic) {
                    return true;
                }
                // Constructor function: a PascalCase `function`, or one
                // referenced via `new`/`.call(this)`/`.apply`/`.bind` or
                // assigned as a method value, gets the instance as `this`.
                if is_constructor_function(func, semantic) {
                    return true;
                }
                // Var-bound function referenced for `this`: an anonymous
                // `function` expression held in a `const`/`let`/`var` whose
                // binding is later invoked via `.bind(this)`/`.call`/`.apply`,
                // with `new`, or assigned as a method value, has its `this`
                // supplied at the binding site (`const localeData = function () {
                // this.$locale() }` called as `localeData.bind(this)()`).
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
                severity: Severity::Warning,
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
    fn allows_this_in_mocha_it_callback() {
        // Regression for #2023: Mocha binds a Test context to `this` inside an
        // `it(name, function() {...})` callback.
        let src = "it('/POST (concurrent)', function () {\n  this.retries(10);\n  return request(server).post('/concurrent').expect(200);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_mocha_describe_skip_callback() {
        // Regression for #2023: `describe.skip(name, function() {...})` with
        // `this.timeout()` / `this.retries()`.
        let src = "describe.skip('Kafka transport', function () {\n  this.timeout(50000);\n  this.retries(10);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_mocha_hook_callback() {
        let src = "before('Start app', function () {\n  this.timeout(10000);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_non_mocha_callback() {
        // A `function` passed to a non-Mocha call gets no context — `this` is
        // still unbound and must fire.
        let diags = run_on("arr.forEach(function () { return this.x; });");
        assert_eq!(diags.len(), 1);
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
    fn allows_this_in_cypress_then_callback() {
        // Regression for #1842: Cypress binds the shared test context to `this`
        // inside a `function` callback passed to `.then()` in a `cy` chain.
        // Aliases registered via `.as('name')` are read as `this.name`.
        let src = "cy.get('div')\n  .contains('animate')\n  .as('spring')\n  .then(function () {\n    const bounds = this.miniDefault[0].getBoundingClientRect();\n  });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_cypress_should_callback() {
        // Regression for #1842: `.should(function() {...})` is the other Cypress
        // chain method that binds the test context to `this`.
        let src = "cy.get('@spring').should(function () {\n  expect(this.value).to.equal(1);\n});";
        assert!(run_on(src).is_empty());
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
    fn flags_this_in_callback_with_this_arg_before_it() {
        // Negative-space guard for #3812: a `this` passed *before* the callback is
        // data, not the `thisArg` (which the spec places after the callback) —
        // `this` in the callback stays unbound and must fire. The leading `this`
        // argument sits in a class method so only the callback's `this` is flagged.
        let src = "class Foo {\n  run() {\n    return foo(this, function () {\n      return this.x;\n    });\n  }\n}";
        let diags = run_on(src);
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
}

//! no-property-mutation OXC backend — flag property mutations.
//!
//! Three Three.js/react-three-fiber imperative-write categories are exempt, as
//! each mutates a stateful renderer-managed instance with no immutable form:
//! the `onBeforeCompile` material hook, browser host-object writes
//! (Location/History, DOM `.style`/`.dataset` chains, `on<event>` handler
//! registration), and in-place scene-object mutation inside a `useFrame`
//! animation callback (`mesh.current.position.y`, `state.camera.position.x`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, is_call_ref_value_target, is_get_context_call_binding,
    is_local_object_builder_binding, is_node_module_system_target, is_owned_fresh_array_binding,
    is_pinia_store_binding,
    is_react_display_name_assignment, is_reassigned_fresh_copy_at, is_reduce_accumulator_param,
    is_rtk_reducer_draft_param, is_sole_owned_fresh_object_at, is_typed_array_binding,
    is_unist_visitor_node_param, is_valtio_proxy_binding, is_vue_deep_reactive_receiver,
    is_vue_directive_hook_element_param, is_vue_reactive_object_target, is_vue_ref_value_target,
    root_identifier_of_expr,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const SENTRY_HOOKS: &[&str] = &["beforeSend", "beforeBreadcrumb", "beforeSendTransaction"];

/// Methods/callbacks whose documented contract is in-place mutation of a handed-in
/// parameter. `onBeforeCompile` is a Three.js material lifecycle hook that receives a
/// `shader` object and configures it by assigning sub-properties (`shader.uniforms`,
/// `shader.defines`) — there is no immutable API.
const MUTATION_HOOK_METHODS: &[&str] = &["onBeforeCompile"];

/// Static name of an object-property key, if it's an identifier or string literal.
fn static_key_name<'a>(key: &PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Name of the nearest enclosing named function (declaration or named expression).
fn nearest_enclosing_fn_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::Function(func) = ancestor.kind()
            && let Some(id) = &func.id
        {
            return Some(id.name.as_str());
        }
    }
    None
}

/// True when `node_id` sits lexically inside a callback assigned to a Sentry
/// hook — an ancestor object property keyed by one of [`SENTRY_HOOKS`].
fn inside_inline_sentry_callback(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    semantic.nodes().ancestors(node_id).any(|ancestor| {
        matches!(ancestor.kind(), AstKind::ObjectProperty(prop)
            if static_key_name(&prop.key).is_some_and(|name| SENTRY_HOOKS.contains(&name)))
    })
}

/// True when the mutation sits inside a Sentry hook — Sentry hands the event to
/// the hook by reference, expects it back mutated, and offers no immutable API,
/// so the write has no alternative form. Three shapes reach the same hook:
///
/// - an inline lambda or method assigned to `beforeSend` / `beforeBreadcrumb` /
///   `beforeSendTransaction`;
/// - a named function registered by reference
///   (`beforeSend: scrubEventRequestUrl`);
/// - a helper the hook CALLS (`beforeBreadcrumb(b) { scrubStringField(b.data,
///   'url') }`), which receives the very same object and must mutate it for the
///   hook to have any effect.
///
/// The call form is followed one hop only, and only within this file: it asks
/// whether the enclosing function is called from inside a Sentry callback, not
/// whether some chain of calls eventually reaches one. One hop is what the
/// documented shape needs — the hook body delegating its scrubbing — and a
/// deeper walk would hand a blanket exemption to any utility a hook happens to
/// touch.
fn is_inside_sentry_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if inside_inline_sentry_callback(node.id(), semantic) {
        return true;
    }
    let Some(fn_name) = nearest_enclosing_fn_name(node, semantic) else {
        return false;
    };
    semantic.nodes().iter().any(|n| match n.kind() {
        AstKind::ObjectProperty(prop) => {
            static_key_name(&prop.key).is_some_and(|name| SENTRY_HOOKS.contains(&name))
                && matches!(&prop.value, Expression::Identifier(id) if id.name.as_str() == fn_name)
        }
        AstKind::CallExpression(call) => {
            matches!(&call.callee, Expression::Identifier(id) if id.name.as_str() == fn_name)
                && inside_inline_sentry_callback(n.id(), semantic)
        }
        _ => false,
    })
}

/// True when the mutation sits inside a method or callback named for a documented
/// in-place-mutation hook (`MUTATION_HOOK_METHODS`). Covers both the class-method
/// shape `class M extends THREE.ShaderMaterial { onBeforeCompile(shader) { … } }`
/// (a `MethodDefinition` keyed by the hook name) and the object-property-keyed
/// callback shape `{ onBeforeCompile() {} }` / `{ onBeforeCompile: (shader) => {} }`.
fn is_inside_mutation_hook_method<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut ancestors = semantic.nodes().ancestors(node.id()).peekable();
    while let Some(ancestor) = ancestors.next() {
        match ancestor.kind() {
            // Class/object method: `onBeforeCompile(shader) { … }` — the method body
            // is a `Function` node wrapped by a `MethodDefinition` keyed by the name.
            AstKind::Function(_) => {
                if let Some(next) = ancestors.peek()
                    && let AstKind::MethodDefinition(method) = next.kind()
                    && static_key_name(&method.key)
                        .is_some_and(|name| MUTATION_HOOK_METHODS.contains(&name))
                {
                    return true;
                }
            }
            // Object property whose value is the hook callback:
            // `{ onBeforeCompile: (shader) => {} }` / `{ onBeforeCompile() {} }`.
            AstKind::ObjectProperty(prop) => {
                if static_key_name(&prop.key)
                    .is_some_and(|name| MUTATION_HOOK_METHODS.contains(&name))
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// True when `node` is inside a function/arrow passed as an argument to a
/// `useFrame(...)` call — react-three-fiber's per-frame animation hook, where
/// in-place mutation of Three.js scene objects (`mesh.current.position.y`,
/// `state.camera.position.x`) is the sole supported animation API: Three.js
/// `Vector3`/`Euler`/etc. are stateful instances with no immutable alternative.
/// The callback is a direct argument of the `CallExpression` (no `Argument`
/// wrapper node), so the enclosing arrow/function's parent is that call.
fn is_inside_useframe_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        if matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) && let AstKind::CallExpression(call) = nodes.parent_node(ancestor.id()).kind()
            && let Expression::Identifier(callee) = &call.callee
            && callee.name.as_str() == "useFrame"
        {
            return true;
        }
    }
    false
}

/// Get the root object identifier name from an expression chain.
fn root_object_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => root_object_name(&m.object),
        Expression::ComputedMemberExpression(m) => root_object_name(&m.object),
        _ => None,
    }
}

/// True when the member-access chain is rooted at `this` (e.g. `this.x`,
/// `this.ctx.counter`). Writing the object's own instance state is encapsulated
/// state with no immutable form, not the external/shared mutation this rule
/// targets.
fn is_rooted_at_this(expr: &Expression) -> bool {
    match expr {
        Expression::ThisExpression(_) => true,
        Expression::StaticMemberExpression(m) => is_rooted_at_this(&m.object),
        Expression::ComputedMemberExpression(m) => is_rooted_at_this(&m.object),
        _ => false,
    }
}

/// True when `object` (the base of a computed-member write `base[i]`) is a direct
/// identifier resolving to a TypedArray binding — `buf[i] = v`, `buf[i]++`. Only
/// the direct indexed write on a TypedArray is exempt; a deeper chain
/// (`obj.buf[i]`) keeps its non-identifier base and stays flagged.
fn is_typed_array_element_object(
    object: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    matches!(
        object,
        Expression::Identifier(id) if is_typed_array_binding(id, semantic)
    )
}

/// True when `ident` resolves to a binding initialised via `document.createElement(...)`
/// or `document.createElementNS(...)`. A freshly created DOM element is unattached and
/// must be configured by property assignment before insertion — not a state mutation.
fn is_created_dom_element(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            let Some(init) = &decl.init else { return false };
            return is_create_element_call(init);
        }
    }
    false
}

const DOM_WRITE_INTERMEDIARIES: &[&str] = &["style", "dataset"];

/// True when the assignment target chain passes through a DOM write property
/// such as `el.style.width = v` or `el.dataset.key = v`. Mutating `.style`/
/// `.dataset` sub-properties is the canonical imperative DOM API with no
/// immutable alternative.
fn has_dom_write_intermediary(expr: &Expression) -> bool {
    match expr {
        Expression::StaticMemberExpression(m) => {
            if DOM_WRITE_INTERMEDIARIES.contains(&m.property.name.as_str()) {
                return true;
            }
            has_dom_write_intermediary(&m.object)
        }
        _ => false,
    }
}

/// True when the assignment target is an imperative browser host-object write
/// that has no immutable/spread equivalent — assigning the property *is* the API:
/// - any `Location` property (`location.href = x`, `window.location.hash = x`, …):
///   every write triggers navigation;
/// - `window.location = x`: assigning `Location` itself navigates;
/// - `history.scrollRestoration` / `window.history.scrollRestoration`: the only
///   writable `History` property (`state`/`length` are read-only).
fn is_imperative_host_write(obj_text: &str, prop_text: &str) -> bool {
    if obj_text == "location" || obj_text == "window.location" {
        return true;
    }
    if obj_text == "window" && prop_text == "location" {
        return true;
    }
    if (obj_text == "history" || obj_text == "window.history")
        && prop_text == "scrollRestoration"
    {
        return true;
    }
    false
}

/// Identifiers that name the ECMAScript global object across browser, worker, and
/// Node scopes. `global` and `globalThis` are the same object under two
/// spellings, and `prefer-global-this` prescribes rewriting one into the other —
/// so treating them differently here would make one rule's fix silence another
/// rule's diagnostic.
const GLOBAL_OBJECT_NAMES: &[&str] = &["window", "self", "globalThis", "global"];

/// True when `object` is *directly* the ECMAScript global object — the identifier
/// `window`/`self`/`globalThis` resolving to no local binding. A write to a direct
/// property of the global object (`window.$x = v`, `window['$x'] = v`,
/// `globalThis.x = v`) declares a global and has no immutable/spread form (the
/// global object cannot be reconstructed), the same host-write class as the
/// Location/History writes in [`is_imperative_host_write`].
///
/// The resolution guard keeps it precise: a shadowing `const window = {}` or a
/// `window` parameter resolves to a symbol, so its property writes stay flagged.
/// Only the direct object is matched — a nested target such as `window.app.cfg = v`
/// has `window.app` (not the global identifier) as its object and stays flagged, as
/// it mutates an ordinary object.
fn is_global_object(object: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    let Expression::Identifier(id) = object else {
        return false;
    };
    GLOBAL_OBJECT_NAMES.contains(&id.name.as_str())
        && crate::oxc_helpers::reference_resolves_to_no_local_binding(id, semantic)
}

/// Ambient host objects whose property writes ARE their API. Each is a live
/// binding into something outside the program — the document tree, the browser's
/// storage backend, the operating-system process — that no expression can
/// reconstruct, so assignment is the only way to write it:
/// `document.title = x`, `localStorage.theme = 'dark'` (spec-equivalent to
/// `setItem`), `process.exitCode = 1` (Node's documented way to set an exit
/// status without tearing the process down before `finally` runs).
const AMBIENT_HOST_OBJECT_NAMES: &[&str] =
    &["document", "localStorage", "sessionStorage", "process"];

/// True when `object` is *directly* one of the ambient host objects in
/// [`AMBIENT_HOST_OBJECT_NAMES`] — the bare identifier resolving to no local
/// binding. The same host-write class as the Location/History writes in
/// [`is_imperative_host_write`] and the `document.cookie` carve-out.
///
/// The resolution guard keeps it precise: a shadowing `const document = {}` or a
/// `localStorage` parameter resolves to a symbol, so its property writes stay
/// flagged. Only the direct object is matched — a nested target such as
/// `document.body.style = v` or `process.env.KEY = v` has an ordinary object
/// (`document.body`, `process.env`), not the ambient identifier, as its object
/// and stays flagged.
fn is_ambient_host_object(object: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    let Expression::Identifier(id) = object else {
        return false;
    };
    AMBIENT_HOST_OBJECT_NAMES.contains(&id.name.as_str())
        && crate::oxc_helpers::reference_resolves_to_no_local_binding(id, semantic)
}

/// True when the assignment registers a DOM-style event handler: the property
/// name has the `on<event>` shape (`onerror`, `onsuccess`, `onupgradeneeded`,
/// `onclick`, …) and the assigned value is a function (or `null` to deregister).
/// Assigning `obj.on<event> = fn` is the canonical imperative event-registration
/// API for browser host objects (`IDBRequest`, `IDBTransaction`, `WebSocket`,
/// `XMLHttpRequest`, DOM elements) — event REGISTRATION, not the object-state
/// mutation this rule targets, and there is no immutable alternative.
///
/// Gating on a function value keeps the exemption tight: a plain state write
/// like `config.onTimeout = 5000` assigns a non-function and stays flagged.
///
/// A chained assignment `a.onX = b.onY = null` parses right-associatively as
/// `a.onX = (b.onY = null)`, so the outer write's RHS is itself an
/// `AssignmentExpression`. The terminal assigned value is resolved by walking the
/// `.right` chain — through explicit parentheses as well, since
/// `a.onX = (b.onY = null)` may also be written that way — before the shape
/// check, so both writes in a chained handler (de)registration are recognised;
/// the on-event property gate still applies to each write's own left-hand
/// property.
fn is_event_handler_registration(prop_text: &str, value: &Expression) -> bool {
    let is_on_event = prop_text.len() > 2
        && prop_text.starts_with("on")
        && prop_text.as_bytes()[2].is_ascii_lowercase();
    if !is_on_event {
        return false;
    }
    let mut terminal = value;
    loop {
        terminal = match terminal {
            Expression::AssignmentExpression(inner) => &inner.right,
            Expression::ParenthesizedExpression(paren) => &paren.expression,
            _ => break,
        };
    }
    matches!(
        terminal,
        Expression::ArrowFunctionExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::NullLiteral(_)
    )
}

/// True when `expr` creates a DOM element: `document.createElement(tag)` or
/// `document.createElementNS(ns, tag)`, however it is wrapped — behind a cast
/// (`<HTMLElement>document.createElement('div')`) or behind the SSR-safe
/// optional chain (`document?.createElement('link')`). Neither wrapper changes
/// which object the call returns.
fn is_create_element_call(expr: &Expression) -> bool {
    let Some(call) = call_of(expr) else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    let Expression::Identifier(obj) = &member.object else { return false };
    if obj.name.as_str() != "document" { return false }
    let method = member.property.name.as_str();
    method == "createElement" || method == "createElementNS"
}

/// React 18 `use()` Thennable introspection fields: a cached promise is augmented
/// with these so React can read settlement state synchronously during render
/// without awaiting. Assigning them on a promise *is* the documented API — there
/// is no immutable alternative. The name-set alone is far too broad (`obj.status`,
/// `obj.value` are ordinary state writes), so it only exempts when the receiver is
/// also provably a promise (see `is_introspectable_promise_target`).
const PROMISE_INTROSPECTION_FIELDS: &[&str] = &["status", "value", "reason"];

/// True when `expr` constructs a promise: `Promise.reject(...)`, `Promise.resolve(...)`,
/// `new Promise(...)`, or `Promise.withResolvers()`. Any `as`/`satisfies` cast wrapper
/// is unwrapped first (`Promise.reject(r) as RejectedPromise<T>`).
fn is_promise_initializer_expression(expr: &Expression) -> bool {
    match expr {
        Expression::TSAsExpression(as_expr) => is_promise_initializer_expression(&as_expr.expression),
        Expression::TSSatisfiesExpression(s) => is_promise_initializer_expression(&s.expression),
        Expression::TSNonNullExpression(n) => is_promise_initializer_expression(&n.expression),
        Expression::ParenthesizedExpression(p) => is_promise_initializer_expression(&p.expression),
        Expression::NewExpression(new) => {
            matches!(&new.callee, Expression::Identifier(id) if id.name.as_str() == "Promise")
        }
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else { return false };
            let Expression::Identifier(obj) = &member.object else { return false };
            obj.name.as_str() == "Promise"
                && matches!(member.property.name.as_str(), "reject" | "resolve" | "withResolvers")
        }
        _ => false,
    }
}

/// True when `ident` resolves to a local binding whose initializer constructs a
/// promise (`is_promise_initializer_expression`). The receiver is provably a
/// promise from its own data flow, no type information required.
fn is_promise_initialized_binding(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            let Some(init) = &decl.init else { return false };
            return is_promise_initializer_expression(init);
        }
    }
    false
}

/// True when the assignment augments a promise with a React `use()` Thennable
/// introspection field (`status`/`value`/`reason`) — `m.object` is a plain
/// identifier resolving to a promise-initialized binding and `prop_text` is one of
/// the introspection fields. Both gates are required: the name-set alone is too
/// broad, and the promise check is structural (initializer data flow), not a
/// type-provenance signal.
fn is_promise_introspection_target(
    object: &Expression,
    prop_text: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    if !PROMISE_INTROSPECTION_FIELDS.contains(&prop_text) {
        return false;
    }
    matches!(
        object,
        Expression::Identifier(id) if is_promise_initialized_binding(id, semantic)
    )
}

/// True when `ident` resolves to a `function` declaration (`function invariant()
/// {}`). Attaching a property to such a callable (`invariant.debug = …`) is the
/// function-as-namespace pattern — building a callable that also carries utility
/// methods, the way Node's `assert.strictEqual` is exposed. There is no immutable
/// alternative: a class needs `new` and an object literal is not callable.
///
/// Restricted to function DECLARATIONS, the unambiguous namespace shape. An
/// arrow/function-expression bound to a `const` (`const g = () => {}`) is NOT
/// matched: that binding equally covers CSF2 story arrows (`const WithArgs =
/// (args) => …; WithArgs.args = {…}`) and ad-hoc callbacks, where the write is an
/// ordinary mutation the rule must still flag.
///
/// The `Function` check is on the declaration node only, never via ancestors: a
/// function PARAMETER's declaration node also has a `Function` ancestor, and a
/// parameter is external state, not a callable namespace.
fn is_function_declaration_binding(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    matches!(semantic.nodes().kind(decl_node_id), AstKind::Function(_))
}

/// Peel the wrappers that do not change what an expression evaluates to:
/// parentheses and the TypeScript casts (`as T`, `<T>x`, `satisfies T`, `x!`).
/// Every origin test in this file matches the SHAPE of an initializer, so it has
/// to see through them — `<HTMLElement>document.createElement('div')` creates the
/// same element as the bare call, and a test that matched the raw node would
/// re-open its false positive for each new wrapper syntax.
fn peel_wrappers<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    loop {
        current = match current {
            Expression::ParenthesizedExpression(p) => &p.expression,
            Expression::TSAsExpression(a) => &a.expression,
            Expression::TSSatisfiesExpression(s) => &s.expression,
            Expression::TSNonNullExpression(n) => &n.expression,
            Expression::TSTypeAssertion(a) => &a.expression,
            _ => return current,
        };
    }
}

/// The call `expr` performs, whether it is written plainly (`document.createElement(x)`)
/// or through an optional chain (`document?.createElement(x)`). `?.` evaluates
/// the very same call whenever the receiver is non-nullish, so it says nothing
/// about the result — but it wraps the call in a `ChainExpression`, which a bare
/// `Expression::CallExpression` match misses. That is why the SSR-safe spelling
/// every DOM-aware library uses loses exemptions the plain spelling keeps.
fn call_of<'a>(expr: &'a Expression<'a>) -> Option<&'a CallExpression<'a>> {
    match peel_wrappers(expr) {
        Expression::CallExpression(call) => Some(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => Some(call),
            _ => None,
        },
        _ => None,
    }
}

/// The DOM element interfaces whose property writes ARE the DOM API. A binding
/// annotated with one of them names a live host node: it cannot be rebuilt by a
/// spread, so `el.className = x` — or the expando `el._ripple = {}` a custom
/// directive keeps per node — has no immutable form to suggest.
const DOM_ELEMENT_TYPE_NAMES: &[&str] =
    &["HTMLElement", "Element", "Node", "EventTarget", "SVGElement"];

/// True when `ty` names a DOM element interface, looking through the unions the
/// DOM APIs force on callers (`HTMLElement | null` from `querySelector`,
/// `EventTarget | null` from `event.currentTarget`).
fn type_is_dom_element(ty: &TSType) -> bool {
    match ty {
        TSType::TSTypeReference(reference) => matches!(
            &reference.type_name,
            TSTypeName::IdentifierReference(id)
                if DOM_ELEMENT_TYPE_NAMES.contains(&id.name.as_str())
        ),
        TSType::TSUnionType(union) => union.types.iter().any(type_is_dom_element),
        _ => false,
    }
}

/// True when `ident` resolves to a binding the type system says is a DOM
/// element: a parameter or variable annotated with one of
/// [`DOM_ELEMENT_TYPE_NAMES`], or a variable initialised through a cast to one
/// (`event.currentTarget as HTMLElement`).
///
/// This keys on the binding's TYPE, where `is_created_dom_element` keys on one
/// origin (`document.createElement`) and `is_vue_directive_hook_element_param`
/// on one syntactic position (the hook's first parameter). A helper the hook
/// calls — `updateRipple(el: HTMLElement, …)` — receives the very same node and
/// needs the very same exemption.
fn is_dom_element_typed_binding(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    match semantic.nodes().kind(scoping.symbol_declaration(sym_id)) {
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .is_some_and(|annotation| type_is_dom_element(&annotation.type_annotation)),
        AstKind::VariableDeclarator(decl) => {
            if let Some(annotation) = &decl.type_annotation
                && type_is_dom_element(&annotation.type_annotation)
            {
                return true;
            }
            decl.init.as_ref().is_some_and(|init| match init {
                Expression::TSAsExpression(cast) => type_is_dom_element(&cast.type_annotation),
                Expression::TSTypeAssertion(cast) => type_is_dom_element(&cast.type_annotation),
                _ => false,
            })
        }
        _ => false,
    }
}

/// True when the write goes through a `prototype` property —
/// `Sub.prototype.constructor = Sub`, `Sub.prototype.run = function () {…}`,
/// `exports.Sub.prototype[k] = fn`.
///
/// Writing through `.prototype` is how ES5 spells a class: it defines the type's
/// method table and restores `constructor` after the prototype is re-pointed. It
/// runs once, at module scope, and mutates no program state — and the spread
/// remediation has no referent, since `{ ...Sub.prototype, constructor: Sub }`
/// does not make `Sub` construct anything.
fn is_prototype_chain_write(object: &Expression) -> bool {
    match object {
        Expression::StaticMemberExpression(m) => {
            m.property.name.as_str() == "prototype" || is_prototype_chain_write(&m.object)
        }
        Expression::ComputedMemberExpression(m) => is_prototype_chain_write(&m.object),
        _ => false,
    }
}

/// True when `expr` evaluates to a RegExp this file can see: a regex literal or
/// a `new RegExp(...)` construction.
fn is_regexp_expression(expr: &Expression) -> bool {
    match peel_wrappers(expr) {
        Expression::RegExpLiteral(_) => true,
        Expression::NewExpression(new_expr) => {
            matches!(&new_expr.callee, Expression::Identifier(id) if id.name.as_str() == "RegExp")
        }
        _ => false,
    }
}

/// True when every element of `iterated` is a regex literal — written inline, or
/// held by a binding whose initializer is such an array.
fn iterates_regexp_literals(iterated: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    fn is_regexp_literal_array(expr: &Expression) -> bool {
        let Expression::ArrayExpression(array) = expr else { return false };
        !array.elements.is_empty()
            && array
                .elements
                .iter()
                .all(|element| matches!(element, ArrayExpressionElement::RegExpLiteral(_)))
    }

    let iterated = peel_wrappers(iterated);
    if is_regexp_literal_array(iterated) {
        return true;
    }
    let Expression::Identifier(ident) = iterated else { return false };
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return decl
                .init
                .as_ref()
                .is_some_and(|init| is_regexp_literal_array(peel_wrappers(init)));
        }
    }
    false
}

/// True when `object` is provably a RegExp: a regex literal written inline, a
/// binding whose initializer is one, or a `for…of` binding over an array of
/// regex literals (`for (const re of MATCHERS) re.lastIndex = 0`).
///
/// The property name alone is never enough — an application object with its own
/// `lastIndex` field is an ordinary state write — so the receiver carries the
/// evidence.
fn is_regexp_receiver(object: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    if is_regexp_expression(object) {
        return true;
    }
    let Expression::Identifier(ident) = object else { return false };
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            // A `for (const re of …)` binding is a declarator with NO
            // initializer — its value comes from the loop head above, so the
            // walk must climb past it instead of concluding "no evidence".
            AstKind::VariableDeclarator(decl) if decl.init.is_some() => {
                return decl.init.as_ref().is_some_and(|init| is_regexp_expression(init));
            }
            AstKind::ForOfStatement(stmt) => return iterates_regexp_literals(&stmt.right, semantic),
            _ => {}
        }
    }
    false
}

/// True when the assignment resets a global regex's match cursor —
/// `matcher.lastIndex = 0`. `lastIndex` is the only handle the language gives on
/// that cursor, and resetting it before a fresh scan is exactly what
/// `regex-no-stateful-global` prescribes as the fix for the bug it reports.
/// Recompiling the regex instead is what `prefer-static-regex` forbids, so
/// flagging this write would leave no way to write the loop clean.
fn is_regexp_last_index_write(
    object: &Expression,
    prop_text: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    prop_text == "lastIndex" && is_regexp_receiver(object, semantic)
}

/// True when `expr` reads the very property the write targets — the member
/// expression itself (`api.setState`), or a binding initialised from it
/// (`const saved = api.setState`).
fn reads_the_written_property(
    expr_span: oxc_span::Span,
    target_text: &str,
    ctx: &CheckCtx,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let text_at = |span: oxc_span::Span| &ctx.source[span.start as usize..span.end as usize];
    semantic.nodes().iter().any(|node| {
        let span = node.kind().span();
        if !expr_span.contains_inclusive(span) {
            return false;
        }
        match node.kind() {
            AstKind::StaticMemberExpression(_) | AstKind::ComputedMemberExpression(_) => {
                text_at(span) == target_text
            }
            AstKind::IdentifierReference(id) => {
                binding_initializer_text(id, ctx, semantic) == Some(target_text)
            }
            _ => false,
        }
    })
}

/// The source text of the initializer of the binding `ident` resolves to, when
/// it is a plain variable declarator.
fn binding_initializer_text<'a>(
    ident: &IdentifierReference,
    ctx: &'a CheckCtx,
    semantic: &oxc_semantic::Semantic,
) -> Option<&'a str> {
    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let AstKind::VariableDeclarator(decl) =
        semantic.nodes().kind(scoping.symbol_declaration(sym_id))
    else {
        return None;
    };
    let init = decl.init.as_ref()?;
    Some(&ctx.source[init.span().start as usize..init.span().end as usize])
}

/// True when the assigned value is a function that closes over the property's
/// OWN previous value:
///
/// ```ts
/// const saved = api.setState
/// api.setState = (v) => { log(v); saved(v) }
/// ```
///
/// This is decoration, not a state change: the new value is defined in terms of
/// the old one, so no copy of the receiver can express it — a spread rebinds a
/// local and every existing holder keeps calling the undecorated original. The
/// shape identifies itself and needs no knowledge of the library that handed the
/// object over.
///
/// Both halves are required. The value must be a function — a plain
/// `config.timeout = 5000` is an ordinary write — and it must read either the
/// property path itself or a binding initialised from it.
fn is_wrapping_decoration(
    target_span: oxc_span::Span,
    value: &Expression,
    ctx: &CheckCtx,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    if !matches!(
        peel_wrappers(value),
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
    ) {
        return false;
    }
    let target_text = &ctx.source[target_span.start as usize..target_span.end as usize];
    reads_the_written_property(value.span(), target_text, ctx, semantic)
}

/// Peel parentheses only, so a cast stays visible to [`is_widening_cast_target`].
fn peel_parens_only<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(p) => peel_parens_only(&p.expression),
        _ => expr,
    }
}

/// True when the write target is cast to a type WIDER than the receiver's own —
/// `(api as StoreApi<S> & StoreDevtools<S>).devtools = …`, `(api as any).x = …`.
///
/// The cast is the author stating, in the type system, that the write installs a
/// capability the receiver's declared type does not have. That is an extension
/// of the object, which no copy can deliver to whoever already holds it. A cast
/// that widens nothing (`(api as Api).setState = …`) makes no such statement and
/// stays flagged.
fn is_widening_cast_target(object: &Expression) -> bool {
    let Expression::TSAsExpression(cast) = peel_parens_only(object) else { return false };
    matches!(
        &cast.type_annotation,
        TSType::TSAnyKeyword(_) | TSType::TSIntersectionType(_)
    )
}

/// True when the root of the write's receiver chain resolves to a function
/// parameter. JavaScript binds parameters by value, so `{ ...obj, prop: value }`
/// rebinds a local the caller never sees: the spread remediation is a silent
/// no-op there. The write is still reported — it may well be a mutation of the
/// caller's object — but under a message that states something true about it.
fn target_is_parameter(object: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    let Some(ident) = root_identifier_of_expr(object) else { return false };
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    matches!(
        semantic.nodes().kind(scoping.symbol_declaration(sym_id)),
        AstKind::FormalParameter(_)
    )
}

/// Every exemption a write inherits from its RECEIVER — the object it writes
/// through — independently of the write's shape (`=`, `+=`, `??=`, `++`) and of
/// whether the property is named (`o.p`) or computed (`o[k]`). Each entry names
/// either an object whose property writes ARE its API, or one the enclosing
/// function provably built and still owns.
///
/// The carve-outs that read the property NAME or the assigned VALUE cannot be
/// decided from the receiver, so they stay with the arm that has them.
fn receiver_is_exempt(
    object: &Expression,
    node: &oxc_semantic::AstNode,
    write_start: u32,
    ctx: &CheckCtx,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let obj_text = &ctx.source[object.span().start as usize..object.span().end as usize];
    // CommonJS module surface: `module.exports.x = …`, `exports.x = …`.
    if obj_text == "module" || obj_text == "exports" {
        return true;
    }
    // Node Module-system object: `mod.loaded = true`, `Module._cache[id] = …` —
    // mutation is the loader contract.
    if is_node_module_system_target(object, semantic) {
        return true;
    }
    // `window.$x = v`, `global.DateTime = v`, `globalThis[k] = v` — a property
    // written directly on the global object declares a global, and the global
    // object cannot be reconstructed, so no immutable form exists.
    if is_global_object(object, semantic) {
        return true;
    }
    // `document.title = x`, `localStorage.theme = 'dark'`, `process.exitCode = 1`
    // — a direct property write on an ambient host object IS that object's API.
    if is_ambient_host_object(object, semantic) {
        return true;
    }
    // `Sub.prototype.run = fn` — ES5 type definition, not program state.
    if is_prototype_chain_write(object) {
        return true;
    }
    // `(api as Api & { dispatch: … }).dispatch = fn` — the cast says the write
    // installs a capability the receiver's own type does not have.
    if is_widening_cast_target(object) {
        return true;
    }
    // Mutating an object's own instance state (`this.out = sink`) is
    // encapsulated state, not the external/shared mutation this rule targets —
    // replacing the whole object is the only "immutable" form, so there is
    // nothing to suggest.
    if is_rooted_at_this(object) {
        return true;
    }
    if is_inside_sentry_hook(node, semantic) || is_inside_mutation_hook_method(node, semantic) {
        return true;
    }
    if root_object_name(object) == Some("set") {
        return true;
    }
    // `list.value[i] = x`, `list.value.done = true` — a write through the array
    // a Vue `ref([])` holds drives reactivity, and reassigning a fresh array
    // instead reallocates and drops the array's reactive identity.
    if let Expression::StaticMemberExpression(inner) = object
        && is_vue_ref_value_target(inner, semantic, ctx.project, ctx.path)
    {
        return true;
    }
    // Pinia store instance: `store.count = x` writes reactive store state
    // through the proxy the `useXStore()` factory returned — the documented
    // state-write API, with no immutable alternative.
    if let Expression::Identifier(base) = object
        && is_pinia_store_binding(base, semantic, ctx.project, ctx.path)
    {
        return true;
    }
    // `const url = new URL(raw); url.pathname = …` — an object this function
    // constructed and still solely owns. `URL`, `TypeError`, `new Foo()`: the
    // spread this rule would suggest copies no own property of them, so the
    // remediation does not exist. The escape walk is what keeps it honest — the
    // exemption ends as soon as the object is handed out.
    if is_sole_owned_fresh_object_at(object, node.id(), semantic) {
        return true;
    }
    if let Some(id) = root_identifier_of_expr(object)
        && (is_vue_directive_hook_element_param(id, semantic)
            || is_created_dom_element(id, semantic)
            || is_dom_element_typed_binding(id, semantic)
            || is_local_object_builder_binding(id, semantic)
            || is_reassigned_fresh_copy_at(id, write_start, semantic)
            || is_reduce_accumulator_param(id, semantic)
            || is_rtk_reducer_draft_param(id, semantic)
            || is_valtio_proxy_binding(id, semantic)
            || is_get_context_call_binding(id, semantic)
            || is_unist_visitor_node_param(id, semantic))
    {
        return true;
    }
    // `el.style.width = v`, `el.dataset.key = v` — the canonical imperative DOM
    // API, with no immutable alternative.
    has_dom_write_intermediary(object)
}

/// The exemptions a COMPUTED write `base[i]` adds to [`receiver_is_exempt`]:
/// they read the base as a container being indexed, which a named property
/// write never is.
fn computed_receiver_is_exempt(
    object: &Expression,
    ctx: &CheckCtx,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // TypedArray element write `buf[i] = v`: indexed assignment is the only way
    // to populate a fixed-length binary buffer — no immutable element-setter,
    // and no spread-then-build form.
    if is_typed_array_element_object(object, semantic) {
        return true;
    }
    // `const out = []; out[i] = i` — filling an array the function created and
    // never lets escape. Ownership is read off the reference graph
    // (`is_local_fresh_array_binding`), not guessed from the index expression: a
    // dynamic index is not evidence of sharing, and `Array(n)` is no less fresh
    // than `[]`. A parameter array, or one an alias hands out, keeps its
    // diagnostic.
    if matches!(object, Expression::Identifier(base) if is_owned_fresh_array_binding(base, semantic))
    {
        return true;
    }
    // `state.list[0] = item` — `reactive()` proxies every nesting level, so an
    // indexed write on `state.list` is intercepted exactly like the property
    // write `state.list[0].done = true`.
    is_vue_deep_reactive_receiver(object, semantic, ctx.project, ctx.path)
}

/// Report a write this rule decided to flag.
///
/// The message names a remediation that exists at the site. A spread rebuilds a
/// value the caller can be handed back, which holds for a local or a captured
/// object — and never for a parameter, where the copy is bound to a local the
/// caller cannot see and the suggested edit silently does nothing.
fn report(
    diagnostics: &mut Vec<Diagnostic>,
    ctx: &CheckCtx,
    span_start: u32,
    object: &Expression,
    semantic: &oxc_semantic::Semantic,
    write: WriteKind,
) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    let message = match (write, target_is_parameter(object, semantic)) {
        (WriteKind::Assignment, false) => "Property mutation — use spread or immutable patterns.",
        (WriteKind::Update, false) => {
            "Property mutation (increment/decrement) — use immutable patterns."
        }
        (WriteKind::Assignment, true) => {
            "Property mutation on a parameter — the caller keeps the original reference, \
             so no copy reaches it; take the value as input and return the new one."
        }
        (WriteKind::Update, true) => {
            "Property mutation (increment/decrement) on a parameter — the caller keeps the \
             original reference, so no copy reaches it; take the value as input and return \
             the new one."
        }
    };
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: "no-property-mutation".into(),
        message: message.into(),
        severity: Severity::Error,
        span: None,
    });
}

/// Which write shape produced a diagnostic, so its message can name the right one.
#[derive(Clone, Copy)]
enum WriteKind {
    Assignment,
    Update,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression, AstType::UpdateExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Test files mutate local fixtures, accumulators, and mock-captured
        // state freely — bounded to the test scope with no non-mutating
        // alternative. Consistent with no-mutation / no-mutating-assign.
        //
        // Storybook CSF2 attaches story metadata (args, storyName, play,
        // parameters, decorators) by assigning named properties on the exported
        // story function — the designed API with no immutable alternative.
        //
        // Benchmark scripts (`benches/`) are auxiliary evaluation inputs — often
        // third-party real-world programs run to measure engine performance — not
        // production application code subject to immutability conventions.
        if ctx.file.path_segments.in_test_dir
            || ctx.file.path_segments.in_storybook
            || ctx.file.in_benchmark_dir()
        {
            return;
        }
        // react-three-fiber `useFrame((state) => …)` is the per-frame animation
        // callback; mutating Three.js scene objects in place
        // (`mesh.current.position.y`, `state.camera.position.x`) is the sole
        // supported animation API — Three.js `Vector3`/`Euler`/etc. are stateful
        // instances with no immutable alternative.
        if is_inside_useframe_callback(node, semantic) {
            return;
        }
        match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                // Component.displayName = "Component" (React naming convention)
                if is_react_display_name_assignment(assign) {
                    return;
                }
                match &assign.left {
                    AssignmentTarget::StaticMemberExpression(m) => {
                        let obj_text = &ctx.source
                            [m.object.span().start as usize..m.object.span().end as usize];
                        let prop_text = m.property.name.as_str();

                        // Vue 3 reactive ref: `count.value = x` drives reactivity.
                        // Also covers a `Ref<T>` a composable call returned
                        // (`const theme = useStorage(k, v); theme.value = x`).
                        if is_vue_ref_value_target(m, semantic, ctx.project, ctx.path)
                            || is_call_ref_value_target(m, semantic)
                        { return; }
                        // Vue 3 reactive() object: `state.n = x` is the idiomatic update.
                        if is_vue_reactive_object_target(m, semantic, ctx.project, ctx.path) { return; }
                        if prop_text == "current" { return; }
                        if obj_text == "document" && prop_text == "cookie" { return; }
                        if is_imperative_host_write(obj_text, prop_text) { return; }
                        // `request.onerror = () => …`, `el.onclick = fn` — DOM-style
                        // event-handler registration, not object-state mutation.
                        if is_event_handler_registration(prop_text, &assign.right) { return; }
                        // `matcher.lastIndex = 0` — resetting a global regex's match
                        // cursor is the remedy `regex-no-stateful-global` prescribes.
                        if is_regexp_last_index_write(&m.object, prop_text, semantic) { return; }
                        // `promise.status = "rejected"`, `promise.reason = r` on a
                        // promise-initialized local — React 18 `use()` Thennable
                        // introspection augmentation, the documented synchronous-read
                        // API with no immutable alternative.
                        if is_promise_introspection_target(&m.object, prop_text, semantic) { return; }
                        // `invariant.debug = …` — attaching a method to a callable
                        // that resolves (via binding data flow) to a function
                        // declaration: the function-as-namespace pattern (cf.
                        // `assert.strictEqual`), with no immutable form (a class
                        // needs `new`, an object literal is not callable).
                        if let Expression::Identifier(id) = &m.object
                            && is_function_declaration_binding(id, semantic) { return; }
                        // `const saved = api.setState; api.setState = (v) => saved(v)`
                        // — decoration, whose new value is defined in terms of the old.
                        if is_wrapping_decoration(m.span, &assign.right, ctx, semantic) { return; }
                        if receiver_is_exempt(&m.object, node, assign.span.start, ctx, semantic) { return; }

                        report(diagnostics, ctx, assign.span.start, &m.object, semantic, WriteKind::Assignment);
                    }
                    AssignmentTarget::ComputedMemberExpression(m) => {
                        let obj_text = &ctx.source
                            [m.object.span().start as usize..m.object.span().end as usize];

                        if let Expression::StringLiteral(key) = &m.expression
                            && is_imperative_host_write(obj_text, key.value.as_str()) { return; }
                        if is_wrapping_decoration(m.span, &assign.right, ctx, semantic) { return; }
                        if computed_receiver_is_exempt(&m.object, ctx, semantic) { return; }
                        if receiver_is_exempt(&m.object, node, assign.span.start, ctx, semantic) { return; }

                        report(diagnostics, ctx, assign.span.start, &m.object, semantic, WriteKind::Assignment);
                    }
                    _ => {}
                }
            }
            AstKind::UpdateExpression(update) => match &update.argument {
                SimpleAssignmentTarget::StaticMemberExpression(m) => {
                    // Vue 3 reactive ref: `count.value++` drives reactivity.
                    // Also covers a `Ref<T>` a composable call returned.
                    if is_vue_ref_value_target(m, semantic, ctx.project, ctx.path)
                        || is_call_ref_value_target(m, semantic)
                    { return; }
                    // Vue 3 reactive() object: `state.incrementedTimes++` is the idiomatic update.
                    if is_vue_reactive_object_target(m, semantic, ctx.project, ctx.path) { return; }
                    if receiver_is_exempt(&m.object, node, update.span.start, ctx, semantic) { return; }

                    report(diagnostics, ctx, update.span.start, &m.object, semantic, WriteKind::Update);
                }
                SimpleAssignmentTarget::ComputedMemberExpression(m) => {
                    if computed_receiver_is_exempt(&m.object, ctx, semantic) { return; }
                    if receiver_is_exempt(&m.object, node, update.span.start, ctx, semantic) { return; }

                    report(diagnostics, ctx, update.span.start, &m.object, semantic, WriteKind::Update);
                }
                _ => {}
            },
            _ => {}
        }
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    fn run_in_storybook_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_storybook: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    fn run_in_benchmark_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_benchmark_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "crypto.js", crate::project::default_static_project_ctx(), &file)
    }

    /// Build a temp project from `(rel_path, source)` pairs, index it so the
    /// cross-file `ImportIndex` is populated, and run the rule on `target_rel`.
    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> Vec<Diagnostic> {
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            if let Some(lang) = Language::from_path(&p) {
                source_files.push(SourceFile { path: p, language: lang });
            }
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::for_test_with_files(&refs);
        let target_path = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let canon = fs::canonicalize(&target_path).unwrap();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        crate::rules::test_helpers::run_oxc_check(&Check, &source, &canon, &project, file)
    }

    #[test]
    fn skips_in_benchmark_file_issue_4797() {
        // Benchmark scripts (`benches/scripts/v8-benches/crypto.js`) are
        // third-party real-world programs run to measure engine performance —
        // auxiliary evaluation inputs, not production code.
        let src = r#"
            var s_box = new Array();
            s_box[0] = 99;
            obj.prop = value;
        "#;
        assert!(run_in_benchmark_file(src).is_empty());
    }

    #[test]
    fn still_flags_property_mutation_in_src_file() {
        // The same mutation in ordinary source is still flagged: the benchmark
        // exemption is scoped to `benches/` files.
        let src = r#"
            obj.prop = value;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_csf2_story_property_assignment_issue_1679() {
        // Storybook CSF2 attaches story metadata via property assignment on the
        // exported story function — the designed API with no immutable
        // alternative.
        let src = r#"
            export const WithArgs = (args) => <Button {...args} />;
            WithArgs.args = { label: 'With args' };
            WithArgs.play = () => { /* interaction test */ };
        "#;
        assert!(run_in_storybook_file(src).is_empty());
    }

    #[test]
    fn still_flags_same_pattern_in_non_story_file() {
        // The same property-assignment pattern in a non-story file is still a
        // mutation: the Storybook exemption is scoped to `.stories.*` files.
        let src = r#"
            export const WithArgs = (args) => renderButton(args);
            WithArgs.args = { label: 'With args' };
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_in_test_file_issue_582() {
        // Tests mutate local fixtures and mock-captured state freely; bounded
        // to the test scope with no non-mutating alternative.
        let src = r#"
            beforeEach(() => {
                config.retries = 3;
                state["count"] = 0;
            });
        "#;
        assert!(run_in_test_file(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_local_object_spread_builder() {
        // Regression for rbaumier/comply#1930 — dnd-kit boundingRectangle:
        // `value` is a fresh local copy via object spread, built up via
        // conditional property assignments before being returned.
        let src = r#"
            export function boundingRectangle(transform, shape, boundingRect) {
                const value = { ...transform };
                if (cond) {
                    value.y = boundingRect.top - shape.boundingRectangle.top;
                } else if (cond2) {
                    value.y = boundingRect.bottom;
                }
                if (cond3) {
                    value.x = boundingRect.left;
                }
                return value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_local_object_literal_builder() {
        let src = r#"
            function build() {
                const value = { a: 1 };
                value.b = 2;
                return value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_object_literal_cast_builder_issue_7654() {
        // Regression for rbaumier/comply#7654 — `{} as T` is a compile-time-only
        // annotation over a fresh object literal, so indexed writes building it up
        // in a loop stay exempt exactly like a bare `{}` builder.
        let src = r#"
            function getColorPalette(colors) {
                const colorPaletteVar = {} as App.Theme.ThemePaletteColor;
                colors.forEach((color) => {
                    colorPaletteVar[color] = `rgb(0 0 0)`;
                });
                return colorPaletteVar;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_assignment_on_function_parameter() {
        // A function parameter is external state, not a local object builder.
        let src = r#"
            function mutate(value) {
                value.x = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_property_assignment_on_cast_external_call_result() {
        // Negative space: peeling the `as T` cast must not over-exempt — the
        // peeled initializer is a plain function call, not a fresh object literal,
        // so it references external state and the mutation stays flagged.
        let src = r#"
            function f() {
                const value = makeObj() as Config;
                value.x = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_property_assignment_on_const_from_external_call() {
        // A `const` initialized from a function call (not an object literal /
        // spread) references external state — mutating it is still flagged.
        let src = r#"
            function mutate() {
                const value = getConfig();
                value.x = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_property_assignment_on_created_dom_element() {
        let src = r#"
            function download(objectUrl: string, filename: string) {
                const anchor = document.createElement("a");
                anchor.href = objectUrl;
                anchor.download = filename;
                anchor.rel = "noopener";
                document.body.append(anchor);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_created_svg_element() {
        let src = r#"
            function build() {
                const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
                svg.id = "chart";
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_this_assignment_in_constructor() {
        // Regression for issue #477: `this.x = value` in a constructor body is
        // field initialisation (including `readonly` fields), not mutation.
        let src = r#"
            class ProblemError extends Error {
                readonly problem: Problem;
                constructor(problem: Problem) {
                    super();
                    this.name = 'ProblemError';
                    this.problem = problem;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_this_assignment_in_method() {
        // Mutating an object's own instance state inside a method is encapsulated
        // state, not the external/shared mutation this rule targets.
        let src = r#"
            class Foo {
                update() { this.value = 1; }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_this_assignment_in_setter_issue_1335() {
        // Regression for issue #1335: a `set x(v)` accessor exists to intercept
        // assignment; its body must mutate state and has no immutable
        // alternative.
        let src = r#"
            class JSONSchemaGenerator {
                get counter() {
                    return this.ctx.counter;
                }
                set counter(value: number) {
                    this.ctx.counter = value;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_direct_this_field_assignment_in_setter() {
        let src = r#"
            class Foo {
                set name(value: string) {
                    this._name = value;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_this_assignment_in_getter() {
        // `this._x = 1` writes the object's own instance state regardless of the
        // enclosing accessor; getter side effects are a separate concern.
        let src = r#"
            class Foo {
                get x() {
                    this._x = 1;
                    return this._x;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    // FRP stream-operator lifecycle state — issue #5854

    #[test]
    fn allows_frp_operator_lifecycle_this_state_issue_5854() {
        // Regression for rbaumier/comply#5854 — xstream/most.js/bacon FRP stream
        // operators store the downstream sink and clear it in their lifecycle
        // methods (`_start`/`_stop`). These are writes to the operator's own
        // instance state, which has no immutable alternative.
        let src = r#"
            class ThrottleOperator<T> implements Operator<T, T> {
                public out: Stream<T> = null as any;
                private id: any = null;
                _start(out: Stream<T>): void {
                    this.out = out;
                    this.ins._add(this);
                }
                _stop(): void {
                    this.ins._remove(this);
                    this.out = null as any;
                    this.id = null;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_frp_prototype_method_this_state_issue_5854() {
        // most.js uses the prototype-method form; inside a `prototype.x = function`
        // body `this` still refers to the operator's own instance state, so the
        // lifecycle writes (`this.current = …`, `this.ended = true`) are not
        // flagged. The two `SwitchSink.prototype.x = fn` method attachments are
        // clean too since #8098: writing through `.prototype` is how ES5 spells
        // a class, and this file's count moved from 2 to 0 with that decision.
        let src = r#"
            SwitchSink.prototype.event = function(t, stream) {
                this.current = new Segment(t, Infinity, this, this.sink);
                this.current.disposable = stream.source.run(this.current);
            };
            SwitchSink.prototype.end = function(t, x) {
                this.ended = true;
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_param_sink_mutation_in_frp_method_issue_5854() {
        // Negative space: the exemption is `this`-rooted. Mutating a handed-in
        // sink/parameter (external, caller-owned state) inside the same lifecycle
        // method stays flagged — that is the mutation the rule exists to catch.
        let src = r#"
            class Op {
                _start(out) {
                    out.active = true;
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_mutation_on_unrelated_const() {
        let src = r#"
            function set(objectUrl: string) {
                const anchor = getAnchorFromDom();
                anchor.href = objectUrl;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Canvas rendering-context property assignment — issue #2277

    #[test]
    fn allows_property_assignment_on_get_context_binding_issue_2277() {
        // Regression for rbaumier/comply#2277 — a CanvasRenderingContext2D from
        // `canvas.getContext('2d')` is an imperative stateful API; setting
        // `fillStyle`/`lineWidth`/etc. is the only way to use it, no immutable
        // alternative exists.
        let src = r#"
            const ctx = canvas.getContext('2d');
            ctx.fillStyle = 'red';
            ctx.lineWidth = 2;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_non_null_get_context_binding() {
        // The issue's exact shape uses a non-null assertion on the call.
        let src = r#"
            const context = canvas.getContext('2d')!;
            context.fillStyle = gradient;
            context.globalAlpha = 0.5;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_mutation_on_ordinary_object_2277() {
        // Negative space: a const not derived from getContext references
        // external state — mutating it stays flagged.
        let src = r#"
            const o = makeThing();
            o.count = 5;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Sentry beforeSend/beforeBreadcrumb in-place scrub hooks — issue #478

    #[test]
    fn allows_mutation_inside_inline_before_send_arrow() {
        let src = r#"
            Sentry.init({
                beforeSend: (event) => {
                    event.request.url = scrubSensitiveQueryFromUrl(url);
                    return event;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_inside_inline_before_breadcrumb_method() {
        let src = r#"
            Sentry.init({
                beforeBreadcrumb(breadcrumb) {
                    breadcrumb.data = sanitize(breadcrumb.data);
                    return breadcrumb;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_in_named_function_registered_as_before_send() {
        let src = r#"
            function scrubEventRequestUrl(event) {
                event.request.url = scrubSensitiveQueryFromUrl(event.request.url);
                return event;
            }
            Sentry.init({ beforeSend: scrubEventRequestUrl });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_subscript_mutation_in_named_function_registered_as_before_breadcrumb() {
        let src = r#"
            function scrubStringField(bag, key) {
                bag[key] = scrubSensitiveQueryFromUrl(bag[key]);
            }
            Sentry.init({ beforeBreadcrumb: scrubStringField });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_mutation_outside_sentry_hook() {
        let src = r#"
            function scrubStringField(bag, key) {
                bag[key] = scrubSensitiveQueryFromUrl(bag[key]);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // In-place mutation hooks (Three.js onBeforeCompile) — issue #2279

    #[test]
    fn allows_mutation_inside_on_before_compile_class_method_issue_2279() {
        // `onBeforeCompile` is a Three.js material lifecycle hook whose sole API is
        // configuring the handed-in `shader` by sub-property assignment.
        let src = r#"
            class M extends THREE.ShaderMaterial {
                onBeforeCompile(shader) {
                    shader.uniforms.tDiffuse = this._t;
                    shader.defines.USE_UV = '';
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_inside_on_before_compile_object_callback() {
        // The hook can also be supplied as a callback on an inline options object
        // passed to a call, mirroring the Sentry `init({ … })` shape.
        let src = r#"
            applyMaterial({
                onBeforeCompile: (shader) => {
                    shader.uniforms.tDiffuse = t;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_param_mutation_in_differently_named_object_callback() {
        // Same inline-object shape with a non-hook key is still flagged: the
        // exemption keys off the callback name, not the object-callback shape.
        let src = r#"
            applyMaterial({
                notAHook: (shader) => {
                    shader.uniforms.tDiffuse = t;
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_param_mutation_in_differently_named_class_method() {
        // The exemption keys off the hook method name, not "is a parameter": a
        // method with any other name mutating its param is still external state.
        let src = r#"
            class M {
                notAHook(shader) {
                    shader.uniforms.x = 1;
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // react-three-fiber useFrame per-frame animation callback — issue #4412

    #[test]
    fn allows_three_object_mutation_inside_useframe_issue_4412() {
        // Regression for rbaumier/comply#4412 — `useFrame` is R3F's per-frame
        // animation hook; mutating Three.js scene-object properties in place is
        // the sole supported API, with no immutable/spread alternative.
        let src = r#"
            function Box() {
                const mesh = useRef(null);
                useFrame((state) => (mesh.current.position.y = Math.sin(state.clock.elapsedTime)));
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_camera_mutation_inside_useframe_block_body() {
        // Block-body `useFrame` mutating the camera the same way.
        let src = r#"
            useFrame((state) => { state.camera.position.x = 1; });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_same_mutation_outside_useframe() {
        // Negative space: the exemption is `useFrame`-scoped, not a blanket
        // `.current` pass — the same write outside a `useFrame` callback flags.
        let src = r#"
            function f() {
                mesh.current.position.y = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_mutation_inside_different_hook_callback() {
        // Negative space: only `useFrame` is exempt — the same mutation inside a
        // different hook callback (`useEffect`) stays flagged.
        let src = r#"
            useEffect(() => { obj.position.y = 1; });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // DOM .style / .dataset chains — issue #750

    #[test]
    fn skips_dom_style_chain_issue_750() {
        // Mutating `.style` sub-properties is the canonical imperative DOM API;
        // no spread/immutable equivalent exists.
        let src = r#"
            function applyStyle(el: HTMLElement, width: number): void {
                el.style.width = `${width}px`;
                elements.floating.style.maxHeight = `${availableHeight}px`;
                el.dataset.key = "value";
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_direct_style_assignment() {
        // Assigning directly to `.style` (replacing the whole object) is a
        // genuine mutation — only sub-property writes via `.style.X` are exempt.
        // The receiver is deliberately untyped: since #8086 an `el: HTMLElement`
        // annotation exempts every write on the binding, which is a different
        // axis from the `.style` intermediary this test pins.
        let src = r#"
            function reset(el): void {
                el.style = someObj;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Imperative browser host writes (Location / History) — issue #3874

    #[test]
    fn skips_imperative_location_and_history_writes_issue_3874() {
        // Assigning these host-object properties IS the browser API — navigation
        // and scroll-restoration side effects with no spread/immutable form.
        let src = r#"
            function go(target) {
                window.location.href = target;
                location.href = target;
                window.location = target;
                window.history.scrollRestoration = "manual";
                history.scrollRestoration = "auto";
            }
        "#;
        assert!(run(src).is_empty());
    }

    // Direct global-object property writes — issue #7758

    #[test]
    fn skips_direct_global_object_property_writes_issue_7758() {
        // Assigning a property directly on the global object declares a global;
        // the global object cannot be reconstructed, so there is no immutable/
        // spread alternative — the same host-write class as Location/History.
        let src = r#"
            window['$message'] = message;
            window.$dialog = dialog;
            globalThis.x = 1;
            self.foo = bar;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_write_on_shadowed_global_issue_7758() {
        // A shadowing local `window` resolves to a binding, not the real global —
        // the write is an ordinary object mutation and stays flagged. (A fresh
        // `{}` initializer is exempt via the unrelated object-builder rule, so the
        // shadow binds a non-fresh value to isolate the resolution guard.)
        let src = r#"
            const window = getWindow();
            window.foo = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_property_write_on_global_named_param_issue_7758() {
        // A `window` parameter resolves to a binding, not the real global.
        let src = r#"
            function f(window) {
                window.foo = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_nested_global_property_write_issue_7758() {
        // `window.app.config = v` mutates `window.app` (an ordinary object), not a
        // direct property of the global object — stays flagged.
        let src = r#"
            window.app.config = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_ordinary_object_property_write_issue_7758() {
        // Negative space: an ordinary local object (non-global name) stays flagged.
        let src = r#"
            const o = getConfig();
            o.foo = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Direct `document` host-object property writes — issue #7769

    #[test]
    fn skips_direct_document_host_object_writes_issue_7769() {
        // Assigning a writable `Document` property (`title`/`body`/`dir`/…) IS the
        // DOM API — setting the tab title or document element has no spread/
        // immutable form, the same host-write class as Location/History.
        let src = r#"
            router.afterEach((to) => {
                document.title = `${to.meta.title} - App`;
            });
            document.body = el;
            document.dir = 'rtl';
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_computed_document_host_object_write_issue_7769() {
        // The bracket form `document['title'] = x` is the same direct host write.
        let src = r#"
            document['title'] = 'Home';
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_write_on_shadowed_document_issue_7769() {
        // A shadowing local `document` resolves to a binding, not the real global —
        // the write is an ordinary object mutation and stays flagged. (A fresh `{}`
        // initializer is exempt via the unrelated object-builder rule, so the
        // shadow binds a non-fresh value to isolate the resolution guard.)
        let src = r#"
            const document = getDoc();
            document.title = 'x';
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_document_property_write_on_named_param_issue_7769() {
        // A `document` parameter resolves to a binding, not the real global.
        let src = r#"
            function f(document) {
                document.title = 'x';
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_nested_document_property_write_issue_7769() {
        // `document.body.style = v` mutates `document.body` (an element), not a
        // direct property of `document` — stays flagged (the object is
        // `document.body`, not the `document` identifier).
        let src = r#"
            document.body.style = 'x';
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_ordinary_object_title_write_issue_7769() {
        // Negative space: the exemption keys off the resolved `document` global, not
        // the property name — an ordinary object's `.title` write stays flagged.
        let src = r#"
            const o = getConfig();
            o.title = 'x';
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // DOM element property write inside a Vue custom-directive hook — issue #7867

    #[test]
    fn skips_dom_write_in_vue_directive_hook_issue_7867() {
        // Regression for rbaumier/comply#7867 — a directive lifecycle hook's first
        // parameter is the bound `HTMLElement`; a directive is the imperative-DOM
        // escape hatch, so `el['hidden'] = true` is the only API, no immutable form.
        let src = r#"
            app.directive('permiss', {
                mounted(el, binding) {
                    if (binding.value && !permiss.key.includes(String(binding.value))) {
                        el['hidden'] = true;
                    }
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_static_dom_write_in_vue_directive_hook_issue_7867() {
        // The static-member form `el.hidden = true` is the same directive-hook
        // element write.
        let src = r#"
            app.directive('focus', {
                mounted: (el) => {
                    el.tabIndex = 0;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_dom_write_in_directives_component_option_issue_7867() {
        // A directive supplied via the `directives: { name: { … } }` component
        // option is the same provenance; its hook's element param is exempt too.
        let src = r#"
            export default {
                directives: {
                    permiss: {
                        mounted(el, binding) {
                            el.hidden = true;
                        },
                    },
                },
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_ordinary_local_named_el_issue_7867() {
        // Negative space: the gate is the directive-hook first-param position, not
        // the receiver name — an ordinary local named `el` stays flagged.
        let src = r#"
            function f() {
                const el = getElement();
                el['hidden'] = true;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_first_param_write_in_directive_hook_issue_7867() {
        // Negative space: the gate is the FIRST parameter (the element). Mutating a
        // later parameter inside the same hook is external state and stays flagged.
        let src = r#"
            app.directive('permiss', {
                mounted(el, extra) {
                    extra.foo = 1;
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_element_write_in_non_directive_call_issue_7867() {
        // Negative space: the provenance gate is a `.directive(name, { … })` call.
        // The same hook shape passed to an unrelated method stays flagged.
        let src = r#"
            registry.register('permiss', {
                mounted(el, binding) {
                    el['hidden'] = true;
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_hook_method_in_directive_call_issue_7867() {
        // Negative space: the gate also requires a real lifecycle-hook name. A
        // non-hook method (`setup`) inside a genuine `.directive(…)` call is not a
        // directive element hook, so its first-param write stays flagged.
        let src = r#"
            app.directive('x', {
                setup(el) {
                    el.foo = 1;
                },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_readonly_history_property_write_3874() {
        // Only `scrollRestoration` is a writable History setter; writing other
        // History properties is a genuine (and invalid) mutation, stays flagged.
        let src = r#"
            history.length = 0;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // DOM-style event-handler registration — issue #6063

    #[test]
    fn skips_indexeddb_event_handler_registration_issue_6063() {
        // Assigning `on<event>` handlers is the canonical imperative IndexedDB /
        // DOM event-registration API — there is no immutable alternative.
        let src = r#"
            const getRequestPromise = <T>(request: IDBRequest<T>): Promise<T> => {
                return new Promise((resolve, reject) => {
                    request.onerror = () => {
                        reject(request.error);
                    };
                    request.onsuccess = () => {
                        resolve(request.result);
                    };
                });
            };
            const req = indexedDB.open(DB_NAME, 1);
            req.onupgradeneeded = () => {
                req.result.createObjectStore(ENTRIES_STORE_NAME);
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_event_handler_function_expression_and_null_deregister_6063() {
        // A `function` expression handler and `null` (deregistration) are both
        // event-registration forms.
        let src = r#"
            function wire(socket, el) {
                socket.onmessage = function (e) { handle(e); };
                el.onclick = null;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_non_function_on_prefixed_property_write_6063() {
        // Negative space: an `on`-prefixed property assigned a non-function value
        // is a plain state write (a config flag), not handler registration.
        let src = r#"
            function configure(config) {
                config.onTimeout = 5000;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_ordinary_state_mutation_6063() {
        // Negative space: ordinary property mutations stay flagged — the
        // exemption is scoped to the `on<event>`-handler shape.
        let src = r#"
            function update(obj, x) {
                obj.count = 5;
                obj.value = x;
            }
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn still_flags_on_prefixed_capitalized_property_write_6063() {
        // Negative space: `on` followed by an uppercase letter (`onState`) is not
        // the lowercase `on<event>` DOM convention — a state write, stays flagged.
        let src = r#"
            function set(obj, fn) {
                obj.onState = fn;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Chained event-handler (de)registration — issue #7756

    #[test]
    fn skips_chained_event_handler_deregistration_issue_7756() {
        // Regression for rbaumier/comply#7756 — tearing down drag listeners with a
        // single chained write. `document.onmousemove = document.onmouseup = null`
        // parses as `document.onmousemove = (document.onmouseup = null)`, so the
        // outer write's RHS is the inner assignment; both resolve to a `null`
        // terminal, so neither write is flagged.
        let src = r#"
            function stopDrag() {
                document.onmousemove = document.onmouseup = null;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_chained_event_handler_function_registration_7756() {
        // A chained function-handler registration is exempt on both writes.
        let src = r#"
            function wire(el) {
                el.onclick = el.onmouseup = function () {};
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_single_event_handler_deregistration_7756() {
        // Existing behaviour preserved: a single `on<event> = null` deregistration.
        let src = r#"
            function stop() {
                document.onmouseup = null;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_chained_non_handler_mutation_7756() {
        // Negative space: a chained assignment whose terminal value is not a
        // function/null and whose target properties are not `on<event>` handlers is
        // a plain state mutation on both writes. The RHS unwrap is scoped to the
        // value-shape check and does not relax the handler-property requirement.
        let src = r#"
            function build(obj) {
                obj.foo = obj.bar = 5;
            }
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn still_flags_plain_non_handler_mutation_7756() {
        // Negative space: a plain non-handler property write stays flagged.
        let src = r#"
            function build(obj) {
                obj.foo = 5;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // React displayName naming convention — issue #1779

    #[test]
    fn allows_display_name_assignment_on_forward_ref_component() {
        // Regression for rbaumier/comply#1779 — setting `displayName` on a
        // forwardRef-wrapped component is the standard React naming convention.
        let src = r#"
            const RadioGroup = React.forwardRef((props, ref) => {
                return <RadioGroupPrimitives.Root ref={ref} {...props} />;
            });
            RadioGroup.displayName = "RadioGroup";
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    #[test]
    fn still_flags_non_string_display_name_assignment() {
        // A call-expression RHS is a computed value, not the React DevTools
        // naming convention, so it stays a flagged property mutation.
        let src = r#"
            RadioGroup.displayName = getName();
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_other_string_property_assignment() {
        let src = r#"
            RadioGroup.label = "RadioGroup";
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_display_name_inherited_from_wrapped_primitive() {
        // Regression for rbaumier/comply#7844 — the Radix/shadcn wrapper pattern
        // inherits `displayName` from the wrapped primitive
        // (`Foo.displayName = Primitive.Foo.displayName`). Property-name identity
        // on both sides is the React DevTools naming convention, not a mutation.
        let src = r#"
            DropdownMenuGroup.displayName = DropdownMenuPrimitive.Group.displayName;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_display_name_from_template_literal() {
        // The HOC naming pattern assigns a template literal.
        let src = r#"
            Component.displayName = `Wrapped(${inner})`;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_display_name_from_non_display_name_member() {
        // A member-access RHS whose property is not `displayName` is not the
        // inherit convention and stays flagged.
        let src = r#"
            RadioGroup.displayName = Bar.someOtherProp;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn leaves_the_delete_operator_to_no_delete_issue_8356() {
        // Boundary for rbaumier/comply#8356 — one operator, one rule: `no-delete`
        // decides every `delete` on a property. Not subscribing to the kind is
        // what makes that true; the engine never dispatches it here.
        assert!(!Check.interested_kinds().contains(&AstType::UnaryExpression));
    }

    // Array.reduce() accumulator — issue #2239

    #[test]
    fn allows_property_mutation_on_reduce_accumulator_issue_2239() {
        // Regression for rbaumier/comply#2239 — pinia mapHelpers: the reduce
        // accumulator is a fresh local object literal passed as the seed; it
        // never escapes until `reduce` returns, so building it up via property
        // assignment is the canonical reduce-to-object pattern.
        let src = r#"
            function build(stores, suffix) {
                return stores.reduce((reduced, useStore) => {
                    reduced[useStore.$id + suffix] = function () {
                        return useStore();
                    };
                    return reduced;
                }, {});
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_mutation_on_non_accumulator_parameter() {
        // Negative space: a normal function parameter is external state, not a
        // reduce accumulator — mutating it stays flagged.
        let src = r#"
            arr.reduce((reduced, item) => {
                item.x = 1;
                return reduced;
            }, {});
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_property_mutation_on_accumulator_of_non_reduce_call() {
        // Negative space: the first parameter of a callback to a non-`.reduce()`
        // call is not a local accumulator; mutating it stays flagged.
        let src = r#"
            arr.forEach((acc, item) => {
                acc.x = item;
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Vue 3 reactive ref `.value` mutation — issue #2164

    #[test]
    fn allows_vue_ref_value_mutation_issue_2164() {
        // Regression for rbaumier/comply#2164 — `ref()` returns a reactive
        // wrapper whose `.value` assignment/update is the intended mutation
        // point that drives Vue's reactivity.
        let src = r#"
            import { ref } from 'vue'
            const count = ref(0)
            const input = ref('')
            function update(e) {
                count.value++
                input.value = e.target.value
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_value_mutation_on_non_call_initialized_const() {
        // Negative space: no call produced `plain`, so it holds no composable
        // ref and the `.value` write stays flagged.
        let src = r#"
            const plain = source;
            plain.value = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_value_property_mutation_on_vue_ref() {
        // Negative space: only `.value` is the reactive mutation point; writing
        // any other property on a ref is still a mutation.
        let src = r#"
            import { ref } from 'vue'
            const r = ref(0);
            r.config = 5;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Vue 3 reactive ref `.value` array indexed write — issue #7856

    #[test]
    fn allows_indexed_write_into_vue_ref_value_array_issue_7856() {
        // Regression for rbaumier/comply#7856 — v3-admin-vite tags-view store:
        // `visitedViews.value[index] = { ...view }` writes an element of the
        // deeply-reactive array a `ref([])` holds. The indexed write drives
        // reactivity exactly like the sibling `visitedViews.value.push(…)`
        // (already exempt); reassigning a fresh array drops reactive identity.
        let src = r#"
            import { ref } from 'vue'
            const visitedViews = ref([])
            const addVisitedView = (view) => {
                const index = visitedViews.value.findIndex(v => v.path === view.path)
                if (index !== -1) {
                    visitedViews.value[index] = { ...view }
                } else {
                    visitedViews.value.push({ ...view })
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_indexed_write_into_non_ref_value_array_issue_7856() {
        // Negative space: the exemption is ref-scoped. `list.value[i] = x` where
        // `list` is not a `ref()` binding is an ordinary indexed write and stays
        // flagged.
        let src = r#"
            const list = getList();
            list.value[0] = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Vue ref destructured from a composable call — issue #4458

    #[test]
    fn allows_value_mutation_on_composable_destructured_ref_issue_4458() {
        // Regression for rbaumier/comply#4458 — `error`/`isLoading` are `Ref<T>`
        // returned by a composable; `.value` assignment is the only way to update
        // a ref regardless of how it was produced.
        let src = r#"
            const { data: image, error, isLoading, isReady } = useCachedRequest(currentDate, getNASAPOD)
            function fetchPOD(date) {
                error.value = undefined
                isLoading.value = true
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_mutation_on_renamed_composable_destructured_ref() {
        // Renamed destructuring (`data: image`) still resolves to the call-
        // destructured binding.
        let src = r#"
            const { data: image } = useThing();
            function f() { image.value = 1; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_mutation_on_awaited_composable_destructured_ref() {
        // The composable call may be awaited: `const { x } = await useThing()`.
        let src = r#"
            const { x } = await useAsyncThing();
            function f() { x.value = 1; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_non_value_property_on_composable_destructured_binding() {
        // Negative space: the exemption is `.value`-restricted, so a non-`.value`
        // property write on a call-destructured binding stays flagged.
        let src = r#"
            const { cfg } = useThing();
            function f() { cfg.enabled = true; }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Vue ref via Ref-typed parameter / imported ref — issue #7603

    #[test]
    fn allows_value_mutation_on_ref_typed_parameter_issue_7603() {
        // A composable receives a `Ref<T>` / `ModelRef<T>` as a parameter; the
        // caller produced the ref, and `.value` assignment is the only way to
        // update it. The parameter's type annotation is the structural signal.
        let src = r#"
            export function useIME(content: ModelRef<string>) {
                content.value = 'x';
            }
            function useNavBase(queryClicks: Ref<number>) {
                queryClicks.value += 1;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_mutation_on_ref_factory_defaulted_parameter_issue_7603() {
        // An annotation-less parameter defaulting to a Vue ref factory call is a
        // `Ref<T>` regardless of the caller's argument.
        let src = r#"
            import { ref } from 'vue'
            function useNavBase(queryClicks = ref(0)) {
                queryClicks.value += 1;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_value_mutation_on_plain_object_typed_parameter_issue_7603() {
        // Negative space: a parameter typed `{ value: number }` is a plain object,
        // not a ref — the ref-type match is on the ref-wrapper name set only, so
        // its `.value =` write is a genuine mutation and stays flagged.
        let src = r#"
            function f(box: { value: number }) {
                box.value = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_value_mutation_on_imported_ref_issue_7603() {
        // `isDark` / `hmrSkipTransition` are refs exported from centralized state
        // modules; a `.value =` write on the imported binding is a reactive write.
        let files = &[
            (
                "logic/dark.ts",
                "import { computed } from 'vue'\nexport const isDark = computed({ get() { return true }, set(_v) {} })",
            ),
            (
                "state/index.ts",
                "import { ref } from 'vue'\nexport const hmrSkipTransition = ref(false)",
            ),
            (
                "composables/useEmbeddedCtrl.ts",
                "import { isDark } from '../logic/dark'\n\
                 import { hmrSkipTransition } from '../state'\n\
                 export function useCtrl(color: string) {\n\
                     isDark.value = color === 'dark';\n\
                     hmrSkipTransition.value = false;\n\
                 }",
            ),
        ];
        assert!(run_on_project(files, "composables/useEmbeddedCtrl.ts").is_empty());
    }

    #[test]
    fn still_flags_value_mutation_on_imported_non_ref_const_issue_7603() {
        // Negative space: an imported const bound to a plain object (not a ref
        // factory) is a real object; its `.value =` write stays flagged.
        let files = &[
            ("state/plain.ts", "export const box = { value: 0 };"),
            (
                "composables/useThing.ts",
                "import { box } from '../state/plain'\n\
                 export function useThing() { box.value = 1; }",
            ),
        ];
        assert_eq!(run_on_project(files, "composables/useThing.ts").len(), 1);
    }

    #[test]
    fn still_flags_value_mutation_on_local_shadow_of_imported_ref_issue_7603() {
        // Negative space: a local binding that shadows an imported ref name is a
        // distinct value, not the ref. The imported-ref exemption resolves the
        // actual binding (not a name match), so the local's `.value =` stays
        // flagged even though a same-named import is a ref.
        let files = &[
            (
                "state/index.ts",
                "import { ref } from 'vue'\nexport const flag = ref(false)",
            ),
            (
                "composables/useThing.ts",
                "import { flag } from '../state'\n\
                 export function useThing() {\n\
                     const flag = source;\n\
                     flag.value = 1;\n\
                 }",
            ),
        ];
        assert_eq!(run_on_project(files, "composables/useThing.ts").len(), 1);
    }

    #[test]
    fn still_flags_value_mutation_on_non_call_destructured_binding() {
        // Negative space: the exemption is call-restricted, so a `.value` write on
        // a binding destructured from a non-call initializer (here an identifier)
        // stays flagged.
        let src = r#"
            const { x } = source;
            function f() { x.value = 1; }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Vue ref bound directly from a composable call — issue #7734

    #[test]
    fn allows_value_mutation_on_non_destructured_composable_ref_issue_7734() {
        // Regression for rbaumier/comply#7734 — `useStorage` (VueUse) returns a
        // `RemovableRef<string>`; `.value =` is the reactive update, exactly as
        // for a destructured composable ref.
        let src = r#"
            const themePalette = useStorage("theme-palette", "blue");
            const device = useStorage("device", "desktop");
            function apply(preset, val) {
                themePalette.value = preset.id;
                device.value = val;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_mutation_on_hand_written_composable_ref_issue_7734() {
        // The exemption keys on the call, not on a known composable name, so a
        // hand-written composable's ref is treated like a VueUse one.
        let src = r#"
            const x = useMyThing();
            function f() { x.value = 1; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_mutation_on_wrapped_composable_ref_issue_7734() {
        // `await` and `as T` preserve the value the call produced.
        let src = r#"
            const x = (await useThing()) as Ref<number>;
            function f() { x.value = 1; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_update_expression_on_composable_ref_issue_7734() {
        // The update handler takes the same exemption as the assignment handler.
        let src = r#"
            const count = useCounter();
            function bump() { count.value++; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_non_value_property_on_non_destructured_composable_binding_issue_7734() {
        // Negative space: the exemption is `.value`-restricted, so another
        // property write on a call-initialised binding stays flagged.
        let src = r#"
            const x = useThing();
            function f() { x.other = 1; }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_value_mutation_on_callback_param_in_call_initializer_issue_7734() {
        // Negative space: `r` is the callback's own parameter, not the value
        // `xs.map(...)` returned, so it does not inherit the declarator's call.
        let src = r#"
            const rows = xs.map((r) => { r.value = 1; return r; });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Vue ref via a writable-Ref-typed binding resolved to a vue import — issue #7757

    #[test]
    fn allows_value_mutation_on_ref_typed_destructured_interface_param_issue_7757() {
        // Regression for rbaumier/comply#7757 — `defaultFormModel` is destructured
        // from a parameter typed by a same-file interface whose member is
        // `Ref<any>` imported from `vue`; `.value =` is the reactive update.
        let src = r#"
            import type { Ref, ComputedRef } from 'vue';
            interface UseFormValuesContext {
                defaultFormModel: Ref<any>;
                getSchema: ComputedRef<FormSchema[]>;
                formModel: Recordable;
            }
            export function useFormValues({ defaultFormModel, getSchema, formModel }: UseFormValuesContext) {
                function initDefault() {
                    const obj: Recordable = {};
                    defaultFormModel.value = obj;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_mutation_on_vue_ref_typed_direct_param_issue_7757() {
        // A directly-annotated `Ref<T>` parameter whose type resolves to a `vue`
        // import is a ref; `.value =` is the reactive update.
        let src = r#"
            import type { Ref } from 'vue';
            function f(x: Ref<number>) {
                x.value = 1;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_mutation_on_vue_shallow_ref_typed_variable_issue_7757() {
        // An annotated variable typed `ShallowRef<T>` from `vue` is a ref even
        // when its initializer is not a factory call.
        let src = r#"
            import type { ShallowRef } from 'vue';
            const x: ShallowRef<any> = something;
            x.value = 1;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_value_mutation_on_writable_computed_ref_typed_param_issue_7757() {
        // `WritableComputedRef` (from `computed({ get, set })`) is writable, so a
        // `.value =` write on it is the reactive update.
        let src = r#"
            import type { WritableComputedRef } from 'vue';
            function f(x: WritableComputedRef<number>) {
                x.value = 1;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_value_mutation_on_computed_ref_typed_param_issue_7757() {
        // Negative space: `ComputedRef` (from `computed(getter)`) is read-only —
        // its `.value` is not assignable, so writing it is a genuine error the
        // rule keeps flagging even though the type resolves to `vue`.
        let src = r#"
            import type { ComputedRef } from 'vue';
            function f(x: ComputedRef<number>) {
                x.value = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_value_mutation_on_non_vue_ref_typed_param_issue_7757() {
        // Negative space: a `Ref<T>` whose type name resolves to a non-`vue`
        // import is a look-alike, not a Vue ref — its `.value =` stays flagged.
        let src = r#"
            import type { Ref } from './my-types';
            function f(x: Ref<number>) {
                x.value = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_value_mutation_on_plain_object_typed_param_issue_7757() {
        // Negative space: a parameter typed `{ value: number }` is a plain object,
        // not a ref-wrapper reference, so its `.value =` write stays flagged.
        let src = r#"
            function f(x: { value: number }) {
                x.value = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Vue 3 reactive() object property mutation — issue #4457

    #[test]
    fn allows_vue_reactive_object_mutation_issue_4457() {
        // Regression for rbaumier/comply#4457 — `reactive()` returns a reactive
        // proxy whose property mutations (`state.n += amount`,
        // `state.incrementedTimes++`) are the idiomatic Pinia setup-store / Vue 3
        // way to drive reactivity, not a plain-object mutation.
        let src = r#"
            import { reactive } from 'vue'
            function f() {
                const state = reactive({ n: 0, incrementedTimes: 0 });
                function increment(amount = 1) {
                    state.incrementedTimes++;
                    state.n += amount;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_vue_shallow_reactive_object_mutation() {
        // `shallowReactive` tracks its root-level properties, so a root-level
        // write follows the same reactive-proxy mutation contract.
        let src = r#"
            import { shallowReactive } from 'vue'
            const state = shallowReactive({ n: 0 });
            state.n = 5;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_nested_member_mutation_on_shallow_reactive_object() {
        // Negative space: `shallowReactive` does no deep conversion — `state.nested`
        // is the raw object, so writing through it drives no reactivity and is a
        // plain-object mutation. The depth of the exemption tracks the factory.
        let src = r#"
            import { shallowReactive } from 'vue'
            const state = shallowReactive({ nested: { n: 0 } });
            state.nested.n = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_reactive_object_mutation_through_as_cast_issue_7654() {
        // Regression for rbaumier/comply#7654 — a `reactive({…}) as T` cast does not
        // change the reactive proxy the factory returns, so property writes stay the
        // idiomatic Vue 3 reactivity-update path, same as an uncast `reactive(…)`.
        let src = r#"
            import { reactive } from 'vue'
            function useTable() {
                const pagination = reactive({ page: 1, pageSize: 10 }) as PaginationProps;
                function setPage(page) {
                    pagination.page = page;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_plain_object_property_mutation() {
        // Negative space: a parameter object is not a reactive proxy — mutating
        // its property stays flagged (the reactive exemption must not leak to
        // non-reactive bindings).
        let src = r#"
            function f(o) {
                o.n = 5;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_local_reactive_not_imported_from_vue() {
        // Negative space: a same-named local `reactive()` (not imported from vue)
        // returns a plain object — mutating its property stays flagged.
        let src = r#"
            function reactive(x) { return x; }
            const s = reactive({ n: 0 });
            s.n = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_nested_member_mutation_on_reactive_object_issue_7719() {
        // Regression for rbaumier/comply#7719 — `reactive()` returns a deeply
        // reactive proxy, so a nested write drives reactivity exactly like a
        // top-level one and has no immutable alternative.
        let src = r#"
            import { reactive } from 'vue'
            const state = reactive({ nested: { n: 0 } });
            state.nested.n = 1;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_deep_member_mutation_on_reactive_object_issue_7719() {
        // The proxy wraps every nesting level, so the exemption follows the chain
        // to any depth — `state.pageable.total = x` in the reporting repro.
        let src = r#"
            import { reactive } from 'vue'
            const s = reactive({ a: { b: { c: 0 } } });
            s.a.b.c = 1;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_computed_link_member_mutation_on_reactive_object_issue_7719() {
        // A computed link in the chain (`s.list[0]`) reads the same deep proxy —
        // the element is wrapped too, so the write stays the reactive-update path.
        let src = r#"
            import { reactive } from 'vue'
            const s = reactive({ list: [{ done: false }] });
            s.list[0].done = true;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_nested_reactive_update_expression_issue_7719() {
        // The update-expression arm exempts deep writes on the same grounds.
        let src = r#"
            import { reactive } from 'vue'
            const state = reactive({ pageable: { pageNum: 1 } });
            state.pageable.pageNum++;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_nested_member_mutation_on_plain_object() {
        // Negative space: the deep-chain walk resolves the root binding, it does
        // not blanket-allow nested writes — a parameter object is not a reactive
        // proxy, so a nested write on it stays flagged.
        let src = r#"
            function f(o) {
                o.a.b = 2;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_nested_member_mutation_behind_a_call() {
        // Negative space: a call breaks the chain — `getState()` returns an
        // unresolvable value that cannot inherit a reactive binding's identity,
        // so the write stays flagged even next to a `reactive()` binding.
        let src = r#"
            import { reactive } from 'vue'
            const state = reactive({ a: { b: 1 } });
            function getState() { return state; }
            getState().a.b = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Pinia store instance direct state write — issue #7857

    #[test]
    fn allows_direct_state_write_on_imported_pinia_store_issue_7857() {
        // Regression for rbaumier/comply#7857 — v3-admin-vite useLayoutMode:
        // `settingsStore.layoutMode = mode` writes reactive state through the
        // store instance the `useSettingsStore()` factory returned.
        // `useSettingsStore` is a `defineStore(...)` exported from a Pinia store
        // module — the documented, only Pinia state-write API.
        let files = &[
            (
                "pinia/stores/settings.ts",
                "import { defineStore } from 'pinia'\n\
                 export const useSettingsStore = defineStore('settings', () => ({}))",
            ),
            (
                "composables/useLayoutMode.ts",
                "import { useSettingsStore } from '../pinia/stores/settings'\n\
                 const settingsStore = useSettingsStore()\n\
                 function setLayoutMode(mode) { settingsStore.layoutMode = mode }",
            ),
        ];
        assert!(run_on_project(files, "composables/useLayoutMode.ts").is_empty());
    }

    #[test]
    fn allows_state_update_on_local_pinia_store_issue_7857() {
        // The store factory and its consumer can live in one module; the
        // update-expression arm takes the same exemption.
        let src = r#"
            import { defineStore } from 'pinia'
            const useCounterStore = defineStore('counter', () => ({}))
            const counter = useCounterStore()
            function inc() { counter.count++ }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_state_write_on_non_pinia_call_binding_issue_7857() {
        // Negative space: `useThing()` does not resolve to a `defineStore(...)`
        // factory, so the property write is an ordinary mutation and stays flagged.
        let src = r#"
            const thing = useThing();
            thing.count = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_state_write_when_define_store_not_from_pinia_issue_7857() {
        // Negative space: `defineStore` imported from a non-pinia module is not
        // Pinia's factory, so the store-instance write stays flagged.
        let src = r#"
            import { defineStore } from './not-pinia'
            const useX = defineStore('x', () => ({}))
            const x = useX();
            x.count = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_module_instance_property_mutation_issue_5256() {
        // jiti's CJS module loader mutates a `Module` instance in place — the
        // module-loader contract. `new Module()` keeps the prototype + cache
        // identity, so a spread alternative is impossible.
        let src = r#"
            import { Module } from "node:module";
            const mod = new Module(filename);
            mod.filename = filename;
            mod.require = _jiti;
            mod.path = dirname(filename);
            mod.paths = Module._nodeModulePaths(mod.path);
            mod.loaded = true;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_module_cache_computed_assignment_issue_5256() {
        // `Module._cache[id] = mod` populates the CJS require cache — the loader
        // contract; `Module` resolves to the node:module builtin.
        let src = r#"
            import { Module } from "node:module";
            Module._cache[id] = mod;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_module_instance_via_require_issue_5256() {
        // Same exemption for a CommonJS `require("module")` binding.
        let src = r#"
            const { Module } = require("module");
            const mod = new Module(filename);
            mod.loaded = true;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_module_lookalike_not_from_node_module_issue_5256() {
        // Negative space: a `new Module()` whose `Module` is a local class (not
        // imported from node:module) is an ordinary object — still flagged once
        // it is handed out. The `register(mod)` call is what makes the write
        // observable; without it #8199's fresh-and-private exemption answers
        // first, and the node:module discrimination would never be reached.
        let src = r#"
            class Module {}
            const mod = new Module(filename);
            register(mod);
            mod.loaded = true;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_ordinary_cache_computed_assignment_issue_5256() {
        // Negative space: a `cache[id] = x` on a foreign object (a parameter, not
        // a local builder) stays flagged — the exemption is keyed on the `Module`
        // builtin, not on a `cache`/`_cache` member name.
        let src = r#"
            function store(cache, id, value) {
                cache[id] = value;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // TypedArray indexed element assignment — issue #5328

    #[test]
    fn allows_typed_array_element_assignment_issue_5328() {
        // Regression for rbaumier/comply#5328 — pdf-lib pdfDocEncoding: a
        // Uint16Array lookup table populated by indexed writes during module
        // init. Indexed assignment is the only way to write a TypedArray's
        // contents; there is no immutable element-setter to suggest.
        let src = r#"
            const pdfDocEncodingToUnicode = new Uint16Array(256);
            for (let idx = 0; idx < 256; idx++) {
                pdfDocEncodingToUnicode[idx] = idx;
            }
            pdfDocEncodingToUnicode[0x16] = toCharCode('^W');
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_typed_array_element_compound_and_update() {
        // Compound assignment (`buf[i] += v`) and update (`buf[i]++`) on a
        // TypedArray element are the same in-place buffer write.
        let src = r#"
            const buf = new Float64Array(8);
            buf[0] += 1.5;
            buf[1]++;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_typed_array_element_assignment_via_type_annotation() {
        // A `: Uint8Array` type annotation is the same TypedArray signal even
        // when the initializer is an opaque call.
        let src = r#"
            const buf: Uint8Array = getBuffer();
            buf[0] = 255;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_plain_array_element_assignment() {
        // Negative space: an element write on an array the function does not own
        // has immutable alternatives (spread, map) — it stays flagged. Ownership
        // is the criterion since #8182, so the receiver is a parameter; a local
        // `new Array(3)` is as fresh as `[]` and is now clean.
        let src = r#"
            function f(arr: number[]): void {
                arr[0] = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_object_property_mutation_alongside_typed_array() {
        // Negative space: the TypedArray exemption must not leak — a plain
        // object property write stays flagged even in a file that also has a
        // TypedArray element write.
        let src = r#"
            const buf = new Uint8Array(4);
            buf[0] = 1;
            obj.x = 2;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Sparse dispatch-table construction — issue #5412

    #[test]
    fn allows_sparse_dispatch_table_construction_issue_5412() {
        // Regression for rbaumier/comply#5412 — y-websocket message handlers: a
        // locally-owned `const handlers = []` populated by constant-keyed indexed
        // assignment to build an O(1) protocol dispatch table. The sparse layout
        // can't be a constructor literal, so indexed assignment is construction,
        // not mutation.
        let src = r#"
            const messageSync = 0
            const messageAwareness = 1
            const messageHandlers = []
            messageHandlers[messageSync] = (encoder, decoder) => {}
            messageHandlers[messageAwareness] = (encoder, decoder) => {}
            messageHandlers[0x02] = (encoder, decoder) => {}
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_param_array_element_write_5412() {
        // Negative space: a function-parameter array is foreign state, not a
        // locally-owned table being constructed — indexed writes stay flagged.
        let src = r#"
            function f(arr) {
                arr[0] = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_indexed_write_on_a_local_array_that_escaped_issue_8182() {
        // Negative space, re-based by #8182: the index shape is not the
        // criterion — `arr[i] = v` on a locally-created array is clean whatever
        // the index looks like. What keeps a diagnostic is an alias handed out,
        // here the `use(arr)` argument, which lets other code observe the fill.
        let src = r#"
            const arr = [];
            use(arr);
            for (let i = 0; i < 3; i++) {
                arr[i] = i;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_object_property_mutation_alongside_dispatch_table_5412() {
        // Negative space: the dispatch-table exemption must not leak — a plain
        // object property write stays flagged alongside a dispatch-table write.
        let src = r#"
            const handlers = [];
            handlers[0] = fn;
            obj.x = 2;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_indexed_write_through_an_alias_of_a_parameter_array_issue_8182() {
        // Negative space, re-based by #8182: a `const` alias of the caller's
        // array is the case the old `const`-and-constant-index proxy let through
        // and the ownership check catches — the initializer names an existing
        // array, so nothing here was freshly allocated.
        let src = r#"
            function fill(shared: number[], i: number): void {
                const alias = shared;
                alias[i] = 0;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Redux Toolkit Immer draft mutations — issue #5596

    #[test]
    fn allows_draft_mutation_in_create_slice_object_reducers_issue_5596() {
        // Classic `reducers: { … }` object form: the `state` first param is an
        // Immer draft; assigning/deleting its properties is the documented RTK
        // update mechanism, not aliased-state mutation.
        let src = r#"
            import { createSlice } from '@reduxjs/toolkit'
            const slice = createSlice({
                name: 'polling',
                initialState,
                reducers: {
                    updatePolling(state, action) {
                        state.apps[action.payload.app] = action.payload.value;
                        state.enabled = true;
                    },
                },
            })
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_draft_mutation_in_create_slice_builder_reducer_issue_5596() {
        // The `reducers: (creators) => ({ … })` builder form wraps the reducer in
        // `creators.reducer((state) => …)`; the draft is still the first param of
        // that nested callback under `createSlice`.
        let src = r#"
            import { createSlice } from '@reduxjs/toolkit'
            const slice = createSlice({
                name: 'polling',
                initialState,
                reducers: (creators) => ({
                    toggleGlobalPolling: creators.reducer((state) => {
                        state.enabled = !state.enabled;
                    }),
                }),
            })
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_draft_mutation_in_create_reducer_builder_add_case_issue_5596() {
        // `createReducer(initial, (builder) => builder.addCase(act, (state) => …))`
        // — the case-reducer callback's first param is the draft.
        let src = r#"
            import { createReducer } from '@reduxjs/toolkit'
            const reducer = createReducer(initialState, (builder) => {
                builder.addCase(increment, (state) => {
                    state.value += 1;
                });
            })
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_draft_typed_parameter_mutation_issue_5596() {
        // The entity-adapter helpers take a `Draft<T>` state by reference and
        // mutate it in place; the `Draft<…>` annotation is the structural signal.
        let src = r#"
            import type { Draft } from 'immer';
            function addOneMutably(entity: T, state: Draft<R>): void {
                state.entities[key] = entity;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_ordinary_parameter_mutation_outside_reducer_issue_5596() {
        // Negative space: a first parameter mutated in an ordinary function (no
        // createSlice/createReducer context, no `Draft<…>` type) stays flagged.
        let src = r#"
            function mutate(state) {
                state.enabled = true;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_immer_draft_typed_parameter_mutation_issue_5596() {
        // Negative space: a `Draft<…>` annotation not imported from `immer` is a
        // same-named domain type, not Immer's draft — mutating it stays flagged.
        let src = r#"
            type Draft<T> = T;
            function edit(doc: Draft<Document>) {
                doc.title = 'x';
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_draft_variable_inside_reducer_issue_5596() {
        // Negative space: a captured outer object mutated inside a reducer is not
        // the draft (not the reducer's first param) — it stays flagged.
        let src = r#"
            import { createSlice } from '@reduxjs/toolkit'
            const cache = getCache();
            const slice = createSlice({
                name: 's',
                initialState,
                reducers: {
                    update(state) {
                        cache.dirty = true;
                    },
                },
            })
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // valtio proxy() reactive mutations — issue #5595

    #[test]
    fn allows_valtio_proxy_property_mutation_issue_5595() {
        // Regression for rbaumier/comply#5595 — valtio's `proxy()` returns a
        // reactive Proxy whose direct mutation IS the API: `state.nested = {…}`
        // and the deep update `state.nested.ticks++` drive reactivity, with no
        // immutable alternative.
        let src = r#"
            import { proxy } from 'valtio'
            const state = proxy<{ number: number; nested?: { ticks: number } }>({ number: 0 })
            state.nested = { ticks: 0 }
            setInterval(() => state.nested && state.nested.ticks++, 200)
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_plain_object_mutation_not_valtio_proxy() {
        // Negative space: a binding from an external call (not initialised by
        // `proxy()` from valtio) is not a reactive proxy — mutating its property
        // stays flagged, even in a file that imports `proxy` from valtio.
        let src = r#"
            import { proxy } from 'valtio'
            const state = proxy({ n: 0 });
            const plain = getConfig();
            plain.n = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_local_proxy_not_imported_from_valtio() {
        // Negative space: a same-named local `proxy()` (not imported from valtio)
        // returns a plain object — mutating its property stays flagged.
        let src = r#"
            function proxy(x) { return x; }
            const state = proxy({ n: 0 });
            state.n = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_promise_introspection_augmentation_issue_6070() {
        // Regression for rbaumier/comply#6070 — apollo-client createRejectedPromise:
        // a promise from `Promise.reject(r)` (behind an `as` cast) is augmented with
        // React 18 `use()` Thennable introspection fields so React reads settlement
        // state synchronously during render — the documented API, no immutable form.
        let src = r#"
            export function createRejectedPromise<TValue = unknown>(reason: unknown) {
                const promise = Promise.reject(reason) as RejectedPromise<TValue>;
                promise.catch(() => {});
                promise.status = "rejected";
                promise.reason = reason;
                return promise;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_introspection_on_resolved_and_with_resolvers() {
        // The exemption covers all promise constructors: `Promise.resolve(...)`,
        // `new Promise(...)`, and `Promise.withResolvers()`, with `.value`.
        let src = r#"
            function cacheResolved(v) {
                const a = Promise.resolve(v);
                a.status = "fulfilled";
                a.value = v;
                const b = new Promise((res) => res(v));
                b.status = "fulfilled";
                b.value = v;
                const { promise: c } = Promise.withResolvers();
                return [a, b, c];
            }
        "#;
        // `c` is destructured (not a direct promise-initialized binding) so it has
        // no introspection write here; `a`/`b` writes are exempt.
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_status_value_on_plain_object_issue_6070() {
        // Strong positive: the introspection name-set on a NON-promise receiver
        // stays flagged — `obj.status`/`obj.value` are ordinary state writes.
        let src = r#"
            function update(obj, item) {
                obj.status = "active";
                item.value = 5;
            }
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn still_flags_status_on_external_call_result() {
        // Strong positive: a `const` from an external call is not a promise
        // initializer — `result.status = ...` stays flagged.
        let src = r#"
            function update() {
                const result = getConfig();
                result.status = "active";
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Function-as-namespace property extension (fn.method = …) — issue #6071

    #[test]
    fn allows_function_namespace_method_attachment_issue_6071() {
        // Regression for rbaumier/comply#6071 — apollo-client's `invariant`:
        // attaching utility methods to a function declaration builds a callable
        // that also carries a namespace (cf. Node's `assert.strictEqual`), the
        // documented API with no immutable form — a class needs `new` and an
        // object literal is not callable.
        let src = r#"
            function invariant(condition: any, message?: string): asserts condition {
                if (!condition) throw new Error(message);
            }
            invariant.debug = wrapConsoleMethod("debug");
            invariant.log   = wrapConsoleMethod("log");
            invariant.warn  = wrapConsoleMethod("warn");
            invariant.error = wrapConsoleMethod("error");
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_mutation_on_non_function_binding_issue_6071() {
        // Strong positive: the receiver must resolve to a function declaration.
        // Ordinary object-state writes on a non-function binding stay flagged
        // (`instance` is an external-call result, `5` is a plain state value).
        let src = r#"
            const instance = makeThing();
            instance.prop = y;
            const count = makeCounter();
            count.total = 5;
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn still_flags_property_mutation_on_arrow_const_binding_issue_6071() {
        // Strong positive: the exemption is restricted to function DECLARATIONS.
        // An arrow bound to a `const` (the CSF2-story / callback shape) stays
        // flagged outside the story-file exemption — `WithArgs.args = {…}` is an
        // ordinary mutation here.
        let src = r#"
            const WithArgs = (args) => renderButton(args);
            WithArgs.args = { label: 'With args' };
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_property_mutation_on_function_parameter_issue_6071() {
        // Strong positive: a function parameter's declaration node has a `Function`
        // ancestor but is NOT a function declaration — it is external state and
        // stays flagged.
        let src = r#"
            function mutate(value) {
                value.x = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_nullish_assignment_after_object_assign_fresh_copy_issue_6076() {
        // Regression for rbaumier/comply#6076 — gqless logger: `options` (a param)
        // is reassigned to a fresh shallow copy via `Object.assign({}, options)`,
        // then defaults are filled in with `??=`/`||=`/`&&=`. The logical
        // assignments mutate the fresh local copy, not the caller's object.
        let src = r#"
            export function createLogger(client, options = {}) {
                options = Object.assign({}, options);
                options.showCache ??= true;
                options.showSelections ??= true;
                options.stringifyJSON ??= false;
                options.label ||= "gqless";
                options.verbose &&= true;
                return options;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_after_object_create_null_fresh_copy() {
        // `Object.assign(Object.create(null), src)` is also a fresh-copy target.
        let src = r#"
            function build(src) {
                let out = Object.assign(Object.create(null), src);
                out.flag = true;
                return out;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_an_object_rest_element_of_a_fresh_copy() {
        // The rest operator allocates an object of its own, so `rest` is a fresh
        // local object even though the pattern reads the copy's properties.
        let src = r#"
            function f(props) {
                const { a, ...rest } = { ...props };
                rest.extra = 1;
                return rest;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_an_object_rest_element_of_a_parameter() {
        // The rest operator allocates whatever it reads from, so `rest` is fresh
        // even though `props` belongs to the caller.
        let src = r#"
            function f(props) {
                const { id, ...rest } = props;
                rest.extra = 1;
                return rest;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_property_assignment_on_a_destructured_property_of_a_parameter() {
        // `nested` names a property of `props`, which the caller still holds.
        let src = r#"
            function f(props) {
                const { nested } = props;
                nested.extra = 1;
                return nested;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_property_assignment_on_a_structured_clone() {
        // `structuredClone` allocates a new object graph, so the write touches
        // nothing the caller holds.
        let src = r#"
            function f(o) {
                const copy = structuredClone(o);
                copy.x = 1;
                return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_after_a_conditional_fresh_reassignment() {
        // A fresh copy one branch builds is enough for this rule, which asks
        // only what writes the binding takes. `no-delete` reads the same
        // classification at full strength and flags the same shape, because the
        // other path writes to the caller's object
        // (`flags_delete_on_a_parameter_conditionally_reassigned_to_a_copy`).
        let src = r#"
            function f(o, c) {
                if (c) { o = { ...o }; }
                o.x = 1;
                return o;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_after_a_module_scope_fresh_reassignment() {
        // Reading writes needs no enclosing function: the program bounds the
        // source-order walk, so a module-level binding is classified like any
        // other.
        let src = r#"
            let options = getDefaults();
            options = { ...options };
            options.x = 1;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_module_scope_reassignment_to_external_state() {
        // Strong positive: at module level too, a write that does not build a
        // fresh copy disqualifies the binding.
        let src = r#"
            let shared = getInitial();
            shared = loadConfig();
            shared.x = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_object_assign_into_existing_object() {
        // Strong positive: `Object.assign(existing, src)` mutates `existing` in
        // place — the receiver is NOT reassigned to a fresh object, so a later
        // property write on it is still a mutation of shared state.
        let src = r#"
            function merge(existing, src) {
                existing = Object.assign(existing, src);
                existing.x = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_property_mutation_on_param_without_fresh_copy_reassignment() {
        // Strong positive: a param that is never reassigned to a fresh copy is
        // external state — `options.x = y` stays flagged.
        let src = r#"
            function configure(options) {
                options.showCache = true;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_mutation_after_reassignment_to_external_state() {
        // Strong positive: a write that does not build a fresh copy disqualifies
        // the binding, so the second mutation stays flagged; the first still
        // sees the fresh copy, because that write comes after it.
        let src = r#"
            function f(options) {
                options = Object.assign({}, options);
                options.a = 1;
                options = getConfig();
                options.b = 2;
            }
        "#;
        // First write (`options.a`) exempt; second (`options.b`) flagged.
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_mutation_before_fresh_copy_reassignment() {
        // Strong positive: a mutation that occurs BEFORE the fresh-copy
        // reassignment still targets the caller's object and stays flagged.
        let src = r#"
            function f(options) {
                options.a = 1;
                options = Object.assign({}, options);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_unist_visitor_node_mutation_by_reference_issue_6065() {
        // remark/unified transformer (logaretm/villus highlight.ts): the visitor
        // is a named function passed by reference to `visit(...)`; mutating the
        // handed-in node in place (`node.value`, `node.type`) is the only
        // AST-transform API the unified ecosystem exposes.
        let src = r#"
            import { visit } from 'unist-util-visit';
            export default function highlight() {
                return function (tree) {
                    visit(tree, 'code', visitor);
                    function visitor(node) {
                        node.value = '<pre>x</pre>';
                        node.type = 'html';
                    }
                };
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_unist_visitor_const_arrow_by_reference_node_mutation() {
        // The unified ecosystem also writes the visitor as a const arrow passed
        // by reference; resolve the visitor name from its binding, not just from
        // a function declaration's own id.
        let src = r#"
            import { visit } from 'unist-util-visit';
            function transform(tree) {
                const visitor = (node) => {
                    node.type = 'html';
                };
                visit(tree, 'code', visitor);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_unist_visitor_inline_node_mutation() {
        // Inline visitor: the node parameter of the arrow handed to `visit(...)`
        // is mutated in place — same AST-transform contract.
        let src = r#"
            import { visit } from 'unist-util-visit';
            function transform(tree) {
                visit(tree, 'code', (node) => {
                    node.type = 'html';
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_unist_visitparents_node_mutation() {
        // `visitParents(...)` is the other unist traversal entry point; mutating
        // the first-param node (`node.tagName`) is the rehype/hast transform API.
        let src = r#"
            import { visitParents } from 'unist-util-visit-parents';
            function transform(tree) {
                visitParents(tree, 'element', (node, ancestors) => {
                    node.tagName = 'div';
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_node_param_mutation_outside_visit_call() {
        // Strong positive: the identical `node.type = …` shape, but the callback
        // is passed to an unrelated function (not visit/visitParents) — the
        // receiver is not a unist visitor node, so it stays flagged.
        let src = r#"
            function transform(tree) {
                forEach(tree, (node) => {
                    node.type = 'html';
                });
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_node_param_mutation_inside_visitor() {
        // Strong positive: inside the visit callback, mutating a closed-over
        // object (`acc`, not the first-param node) is ordinary shared-state
        // mutation, not the AST-transform contract — stays flagged.
        let src = r#"
            import { visit } from 'unist-util-visit';
            function transform(tree, acc) {
                visit(tree, 'code', (node) => {
                    acc.count = 1;
                });
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Assignment/update frontier with no-mutation — issue #8441

    #[test]
    fn one_property_assignment_draws_one_diagnostic_issue_8441() {
        // Regression for rbaumier/comply#8441: `no-mutation` and
        // `no-property-mutation` both used to report every property write, at the
        // same line:column, with two ids and two remediations. This rule owns the
        // assignment axis outright now. A rule-scoped test cannot see a second
        // rule subscribing to the same kind, and `lint_in_memory` runs no
        // `dedup_mutation_family` pass, so this asserts one step before the CLI
        // collapses anything.
        let source = r#"
export function m3(x: { a: number }): void {
  const alias = x;
  alias.a = 2;
  alias.a++;
}
"#;
        let diagnostics = crate::engine::lint_in_memory(
            std::path::Path::new("b.ts"),
            crate::files::Language::TypeScript,
            source,
            crate::config::default_static_config(),
            None,
        );
        let on_line = |line: usize| {
            let mut ids: Vec<&str> = diagnostics
                .iter()
                .filter(|d| d.line == line)
                .map(|d| d.rule_id.as_ref())
                .collect();
            ids.sort_unstable();
            ids
        };
        assert_eq!(on_line(4), ["no-property-mutation"], "assignment: {diagnostics:?}");
        assert_eq!(on_line(5), ["no-property-mutation"], "update: {diagnostics:?}");
    }

    #[test]
    fn allows_ref_current_assignment_issue_8441() {
        // Carried over from `no-mutation`'s retired assignment arm: `ref.current`
        // is React's documented mutable box, whose whole purpose is assignment.
        assert!(run("const ref = useRef(null); ref.current = node;").is_empty());
    }

    // Freshly-constructed non-escaping local — issue #8199

    #[test]
    fn allows_property_assignment_on_a_constructed_url_issue_8199() {
        // Regression for rbaumier/comply#8199 — hono trailing-slash middleware:
        // `new URL(raw)` allocates a value nobody else holds, and a URL has zero
        // own properties, so the prescribed `{ ...url, pathname: x }` copies
        // nothing and stringifies to "[object Object]".
        let src = r#"
            export function trimTrailingSlash(raw: string): string {
              const url = new URL(raw)
              url.pathname = url.pathname.substring(0, url.pathname.length - 1)
              return url.toString()
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_error_code_augmentation_before_throw_issue_8199() {
        // `error.code = 'ERR_*'` before `throw` is the Node convention and the
        // only form that keeps the stack: `{ ...err }` copies neither `message`
        // nor `stack` (both non-enumerable) and is not `instanceof Error`.
        let src = r#"
            export function parseIp(v: string): number {
              const n = Number(v)
              if (Number.isNaN(n)) {
                const error = new TypeError('Invalid IP address')
                error.code = 'ERR_INVALID_IP'
                throw error
              }
              return n
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_a_locally_constructed_class_instance_issue_8199() {
        // No constructor name list: `new` is the evidence, so a user-defined
        // class configured by assignment is exempt without appearing anywhere.
        let src = r#"
            class Foo { name = '' }
            export function build(): Foo {
              const foo = new Foo()
              foo.name = 'x'
              return foo
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_assignment_on_a_parameter_issue_8199() {
        // Control: mutating a caller-provided object stays flagged — that is the
        // rule's core case, and widening what counts as fresh must not touch it.
        assert_eq!(run("export function taint(o: { a: number }): void { o.a = 1 }").len(), 1);
    }

    // Indexed writes filling a locally-created array — issue #8182

    #[test]
    fn allows_indexed_fill_of_a_local_array_issue_8182() {
        // Regression for rbaumier/comply#8182 — jsdiff `bestPath`/KMP table: an
        // array the function creates, fills by index and returns is never
        // observable elsewhere, and the immutable form is an O(n²) rebuild of the
        // very lookup table that makes the algorithm linear.
        let src = r#"
            export function fill(n: number): number[] {
              const out = [];
              for (let i = 0; i < n; i++) {
                out[i] = i * 2;
              }
              return out;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_indexed_fill_of_a_preallocated_array_issue_8182() {
        // `Array(n)` is no less fresh than `[]` — it is the same allocation with
        // a length. The dynamic index carries no information about sharing.
        let src = r#"
            export function failureTable(b: string, endB: number): number[] {
              const map = Array(endB);
              let k = 0;
              map[0] = 0;
              for (let j = 1; j < endB; j++) {
                map[j] = k;
              }
              return map;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_compound_indexed_write_on_a_local_array_issue_8182() {
        // `parts[parts.length - 1] += w` is a compound write on a computed
        // member; it takes the same ownership path as the plain `=` form.
        let src = r#"
            export function coalesce(words: string[]): string[] {
              const parts: string[] = [];
              for (const w of words) {
                if (parts.length) {
                  parts[parts.length - 1] += w;
                } else {
                  parts.push(w);
                }
              }
              return parts;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_indexed_write_on_a_let_bound_local_array_issue_8182() {
        // `let` binds the same fresh array `const` would: ownership decides, not
        // the declaration keyword.
        let src = r#"
            export function fill(n: number): number[] {
              let out = [];
              for (let i = 0; i < n; i++) { out[i] = i; }
              return out;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_indexed_write_on_an_array_an_alias_escaped_issue_8182() {
        // Negative space: passing the array to a call hands out an alias, so the
        // later write is observable and keeps its diagnostic.
        let src = r#"
            export function leak(): number[] {
              const t = [];
              use(t);
              t[0] = 1;
              return t;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_indexed_write_on_a_parameter_array_issue_8182() {
        // Negative space: a caller's array reached through a parameter is exactly
        // the shared-state write this rule exists for.
        assert_eq!(
            run("export function foreign(shared: number[], i: number): void { shared[i] = 0; }").len(),
            1
        );
    }

    #[test]
    fn still_flags_indexed_write_through_a_member_chain_issue_8182() {
        // Negative space: `obj.table[k]` has a non-identifier base, so no binding
        // carries ownership evidence.
        assert_eq!(run("const obj = getObj(); obj.table[k] = v;").len(), 1);
    }

    // DOM origin behind a cast or an optional chain — issues #8066, #8289

    #[test]
    fn allows_dom_write_on_an_angle_bracket_cast_created_element_issue_8066() {
        // Regression for rbaumier/comply#8066 — vue-next-admin `utils/loading.ts`:
        // a cast is a no-op at run time and does not change the origin object.
        let src = r#"
            const div = <HTMLElement>document.createElement('div');
            div.innerHTML = htmls;
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    #[test]
    fn allows_canvas_context_write_behind_a_cast_issue_8066() {
        // vue-next-admin `utils/watermark.ts`: the cast wraps `getContext`, whose
        // binding the rule already exempts un-cast.
        let src = r#"
            const can = document.createElement('canvas');
            const cans = can.getContext('2d') as CanvasRenderingContext2D;
            cans.font = '12px Vedana';
            cans.textBaseline = 'middle';
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_dom_write_on_an_optionally_chained_created_element_issue_8289() {
        // Regression for rbaumier/comply#8289 — vueuse `useFavicon`:
        // `document?.createElement('link')` is how every SSR-aware library writes
        // DOM access, and `?.` does not change what the call returns.
        let src = r#"
            declare const document: any
            export function f(): unknown {
              const link = document?.createElement('link')
              link.rel = 'icon'
              return link
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_dom_write_on_a_chained_and_cast_created_element_issue_8289() {
        // Both wrappers at once.
        let src = r#"
            declare const document: any
            const el = document?.createElement('div') as HTMLDivElement
            el.id = 'x'
        "#;
        assert!(run(src).is_empty());
    }

    // Provably-allocating global calls — issue #8289

    #[test]
    fn allows_property_assignment_on_a_json_parse_result_issue_8289() {
        // Regression for rbaumier/comply#8289 — vueuse `scripts/utils.ts`:
        // ECMA-262 requires `JSON.parse` to construct every object it returns, so
        // no other reference to it exists or can exist. The prescribed
        // `{ ...JSON.parse(raw) }` only allocates a second object.
        let src = r#"
            export function b(raw: string, version: string): string {
              const pkg = JSON.parse(raw)
              pkg.version = version
              pkg.type = 'module'
              return JSON.stringify(pkg)
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_structured_clone_and_from_entries_issue_8289() {
        // The two other spellings of "this call built the object": a deep clone
        // and an entries-to-object conversion. Both sit inside a function, which
        // is where the ownership walk can see every holder — a module-level
        // binding is read by importers this scan cannot enumerate.
        let src = r#"
            export function build(input: object, pairs: [string, number][]): object {
              const a = structuredClone(input)
              a.x = 1
              const b = Object.fromEntries(pairs)
              b.x = 1
              return { a, b }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_assignment_on_a_local_parse_call_issue_8289() {
        // Negative space: the predicate is anchored on the global `JSON`, not on
        // the method name — a local `parse` may hand back a cached value.
        let src = r#"
            import { parse } from 'yaml';
            const o = parse(raw);
            o.x = 1;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Local factory returning a fresh object literal — issue #8443

    #[test]
    fn allows_property_assignment_after_a_local_fresh_object_factory_issue_8443() {
        // Regression for rbaumier/comply#8443 — TanStack table
        // `createPaginatedRowModel`: both branches assign an object allocated in
        // this call, one by a literal and one by a local function whose only
        // `return` is a literal.
        let src = r#"
            function build(src: { rows: number[] }): { rows: number[]; flat: number[] } {
              return { rows: src.rows, flat: [] };
            }
            export function paginate(src: { rows: number[] }, expanded: boolean) {
              let model: { rows: number[]; flat: number[] };
              if (expanded) {
                model = build(src);
              } else {
                model = { rows: src.rows, flat: [] };
              }
              model.flat = [];
              return model;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_when_one_return_of_the_callee_is_not_fresh_issue_8443() {
        // Negative space: EVERY return must allocate, or the call may hand back
        // an object the caller already holds.
        let src = r#"
            function build(cached: { a: number }, useCache: boolean): { a: number } {
              if (useCache) { return cached; }
              return { a: 1 };
            }
            export function f(cached: { a: number }): { a: number } {
              const model = build(cached, true);
              model.a = 2;
              return model;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_an_imported_callee_issue_8443() {
        // Negative space: a cross-file callee has no readable body here.
        let src = r#"
            import { build } from './build';
            export function f() {
              const model = build();
              model.a = 2;
              return model;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_a_self_recursive_callee_and_terminates_issue_8443() {
        // Negative space: the cycle break makes a self-recursive factory foreign
        // rather than looping the walk.
        let src = r#"
            function build(n: number): { a: number } {
              return build(n - 1);
            }
            export function f() {
              const model = build(2);
              model.a = 2;
              return model;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_an_async_callee_returning_an_object_literal_issue_8443() {
        // Negative space: an `async` function returns a promise, never the object
        // literal its body writes.
        let src = r#"
            async function build(): Promise<{ a: number }> {
              return { a: 1 };
            }
            export function f() {
              const model = build();
              model.a = 2;
              return model;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Node global and Web Storage host objects — issue #8168

    #[test]
    fn allows_writes_on_the_node_global_object_issue_8168() {
        // Regression for rbaumier/comply#8168 — luxon `scripts/bootstrap.js`:
        // `global.X = v` is the only way to declare a Node global, and
        // `prefer-global-this` prescribes rewriting it to `globalThis.X = v`,
        // which this rule already accepts. Treating the two spellings differently
        // makes one rule's fix silence another's diagnostic.
        assert!(run("import { DateTime } from 'luxon'; global.DateTime = DateTime;").is_empty());
    }

    #[test]
    fn allows_web_storage_property_writes_issue_8168() {
        // Property assignment on a `Storage` object is spec-defined and
        // equivalent to `setItem`; `{ ...localStorage, theme: 'dark' }` produces
        // a detached plain object and persists nothing.
        let src = r#"
            export function persist(isDark) {
              localStorage.theme = isDark ? "dark" : "light";
              sessionStorage.lastSeen = "now";
              localStorage["mode"] ??= "auto";
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.js").is_empty());
    }

    #[test]
    fn still_flags_writes_on_a_shadowed_global_or_storage_issue_8168() {
        // Negative space: the resolution guard means a local binding of the same
        // name is an ordinary object. Both receivers are parameters, so the
        // verdict comes from the guard and not from a freshness exemption.
        assert_eq!(run("function f(global) { global.x = 1; }").len(), 1);
        assert_eq!(run("function f(localStorage) { localStorage.x = 1; }").len(), 1);
    }

    #[test]
    fn still_flags_nested_writes_under_a_global_or_storage_issue_8168() {
        // Negative space: only a DIRECT property of the ambient object is its
        // API. `global.app` and `localStorage.a` are ordinary objects.
        assert_eq!(run("global.app.cfg = v; localStorage.a.b = v;").len(), 2);
    }

    // Process exit status — issue #8489

    #[test]
    fn allows_process_exit_code_write_issue_8489() {
        // Regression for rbaumier/comply#8489: `process.exitCode = 1` is Node's
        // documented way to set an exit status without terminating immediately.
        // The only alternative, `process.exit(1)`, tears the process down before
        // `finally` runs and leaks whatever it was closing.
        assert!(run("export function fail(): void { process.exitCode = 1; }").is_empty());
    }

    #[test]
    fn still_flags_nested_process_env_write_issue_8489() {
        // Negative space: `process.env` is an ordinary object, and writing a key
        // on it is the shared-state mutation the rule targets.
        assert_eq!(run("process.env.NODE_ENV = 'test';").len(), 1);
    }

    // RegExp match-cursor reset — issue #8106

    #[test]
    fn allows_regex_last_index_reset_issue_8106() {
        // Regression for rbaumier/comply#8106 — libphonenumber-js
        // `PhoneNumberMatcher`: `lastIndex` is the only handle on a global
        // regex's match cursor, and `regex-no-stateful-global` prescribes this
        // very write as its own remedy.
        let src = r#"
            const MATCHERS = [/a(b)/g, /c(d)/g]
            export function findAll(text) {
              const out = []
              for (const matcher of MATCHERS) {
                matcher.lastIndex = 0
                let m
                while ((m = matcher.exec(text))) { out.push(m[1]) }
              }
              return out
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.js").is_empty());
    }

    #[test]
    fn allows_last_index_reset_on_a_constructed_regexp_issue_8106() {
        assert!(run("const re = new RegExp(src, 'g'); re.lastIndex = 0;").is_empty());
    }

    #[test]
    fn still_flags_last_index_write_on_a_plain_object_issue_8106() {
        // Negative space: the property name alone is not evidence — an app object
        // with its own `lastIndex` field is an ordinary state write. The receiver
        // is a parameter, so the fresh-local-object exemption cannot answer first.
        assert_eq!(run("function f(cursor) { cursor.lastIndex = 5; }").len(), 1);
    }

    // ES5 prototype wiring — issue #8098

    #[test]
    fn allows_es5_prototype_wiring_issue_8098() {
        // Regression for rbaumier/comply#8098 — libphonenumber-js `index.cjs.js`:
        // `Ctor.prototype.constructor = Ctor` and `Ctor.prototype.m = fn` are the
        // pre-`class` spelling of a method table. `{ ...Sub.prototype,
        // constructor: Sub }` does not make `Sub` construct anything.
        let src = r#"
            function Base() { this.x = 1 }
            export function Sub(text) { return Base.call(this, text) }
            Sub.prototype = Object.create(Base.prototype, {})
            Sub.prototype.constructor = Sub
            Sub.prototype.run = function() { return 1 }
            exports.Sub.prototype.constructor = exports.Sub
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.js").is_empty());
    }

    #[test]
    fn still_flags_property_write_on_a_non_prototype_receiver_issue_8098() {
        assert_eq!(run("const instance = getInstance(); instance.someProp = 1;").len(), 1);
    }

    // DOM element reached by type rather than by origin — issue #8086

    #[test]
    fn allows_expando_write_on_an_html_element_parameter_issue_8086() {
        // Regression for rbaumier/comply#8086 — vue-pure-admin ripple directive:
        // per-element state stored as an expando on a live DOM node. The helper
        // the hook calls receives the very node the hook's own parameter carries,
        // which the rule already exempts.
        let src = r#"
            function updateRipple(el: HTMLElement, enabled: boolean) {
              el._ripple = el._ripple ?? {};
              el._ripple.enabled = enabled;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_expando_write_on_an_event_target_cast_issue_8086() {
        let src = r#"
            function rippleHide(e: Event) {
              const element = e.currentTarget as HTMLElement | null;
              element._ripple.touched = false;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_write_on_a_plain_typed_parameter_issue_8086() {
        // Negative space: the discriminator is the DOM type, not the parameter
        // position.
        assert_eq!(run("function f(o: { a: number }) { o.a = 1; }").len(), 1);
    }

    // Parenthesized chained handler (de)registration — issue #7824

    #[test]
    fn skips_parenthesized_chained_event_handler_deregistration_issue_7824() {
        // Regression for rbaumier/comply#7824: the RHS unwrap walks the `.right`
        // chain, and explicit parentheses around the inner assignment are part of
        // that chain.
        assert!(run("document.onmousemove = (document.onmouseup = null);").is_empty());
    }

    // Computed-member writes on a Vue reactive() object — issue #7791

    #[test]
    fn allows_computed_write_on_a_reactive_object_issue_7791() {
        // Regression for rbaumier/comply#7791: `reactive()` converts every
        // nesting level, so `state.list` is itself a proxy and an indexed write
        // on it is intercepted exactly like the already-exempt static form.
        let files = &[(
            "src/a.ts",
            "import { reactive } from 'vue'\n\
             const state = reactive({ list: [{ done: false }] })\n\
             state.list[0].done = true\n\
             state.list[0] = newItem\n",
        )];
        assert!(run_on_project(files, "src/a.ts").is_empty());
    }

    #[test]
    fn still_flags_computed_write_below_a_shallow_reactive_root_issue_7791() {
        // Negative space: `shallowReactive()` proxies the root only, so a write
        // below it drives no reactivity and is an ordinary mutation.
        let files = &[(
            "src/a.ts",
            "import { shallowReactive } from 'vue'\n\
             const state = shallowReactive({ list: [{ done: false }] })\n\
             state.list[0] = newItem\n",
        )];
        assert_eq!(run_on_project(files, "src/a.ts").len(), 1);
    }

    // Decoration of a handed-in object, and the remediation on a parameter —
    // issue #8262

    #[test]
    fn allows_save_then_replace_decoration_issue_8262() {
        // Regression for rbaumier/comply#8262 — zustand middleware: the new value
        // closes over the old one, so no copy can express it. Applying the spread
        // remediation leaves every existing subscriber on the undecorated store.
        let src = r#"
            type Api = { setState: (v: number) => void }
            export const withLogging = (api: Api): void => {
              const saved = api.setState
              api.setState = (v) => { console.log('set', v); saved(v) }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_capability_installation_through_a_widening_cast_issue_8262() {
        // The intersection cast at the write site states, in the type system,
        // that the write adds a capability the parameter's own type lacks.
        let src = r#"
            type Api = { setState: (v: number) => void }
            export const withDispatch = (api: Api): void => {
              ;(api as Api & { dispatch: (a: string) => void }).dispatch = (a) => api.setState(a.length)
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_a_plain_replacement_on_a_parameter_issue_8262() {
        // Negative space: no prior read and no widening cast — an ordinary
        // overwrite of the caller's object.
        let src = r#"
            type Api = { setState: (v: number) => void }
            export const clobber = (api: Api): void => {
              api.setState = () => {}
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_a_cast_that_widens_nothing_issue_8262() {
        let src = r#"
            type Api = { setState: (v: number) => void }
            export const clobber = (api: Api): void => {
              ;(api as Api).setState = () => {}
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn parameter_targets_do_not_carry_the_spread_remediation_issue_8262() {
        // A parameter is bound by value, so `{ ...obj, prop: value }` rebinds a
        // local the caller never sees. The write stays reported; the message must
        // not name an edit that provably does nothing.
        let diagnostics = run("function f(o: { a: number }) { o.a = 1; o.a++; }");
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(
            diagnostics.iter().all(|d| !d.message.contains("spread")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn local_targets_keep_the_spread_remediation_issue_8262() {
        let diagnostics = run("const o = getConfig(); o.a = 1;");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].message.contains("spread"), "{diagnostics:?}");
    }

    // Sentry scrub helpers — issue #478

    #[test]
    fn allows_in_place_scrub_in_a_helper_the_hook_calls_issue_478() {
        // Regression for rbaumier/comply#478: `scrubStringField` receives the
        // breadcrumb's own bag by reference from the hook. Rebuilding it would
        // change nothing the SDK reads, since the hook returns the object it was
        // handed.
        let src = r#"
            function scrubStringField(bag, key) {
              const value = bag[key];
              if (typeof value === 'string') {
                bag[key] = scrub(value);
              }
            }

            Sentry.init({
              beforeBreadcrumb(breadcrumb) {
                scrubStringField(breadcrumb.data, 'url');
                return breadcrumb;
              },
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_the_same_helper_when_no_hook_calls_it_issue_478() {
        // Negative space: the exemption comes from the call site, not from the
        // helper's shape. Nothing registers a Sentry hook here.
        let src = r#"
            function scrubStringField(bag, key) {
              bag[key] = scrub(bag[key]);
            }
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Strength of the fresh-copy evidence — issue #8444

    #[test]
    fn flags_property_assignment_in_a_branch_mutually_exclusive_with_the_copy_issue_8444() {
        // Regression for rbaumier/comply#8444: the only path reaching `o.x = 1`
        // is the one that did NOT copy, so the write lands on the caller's
        // object. "A fresh copy exists somewhere earlier" was the whole test
        // before; it is now "and the mutation can reach it".
        let src = r#"
            export function g(o: Record<string, number>, c: boolean) {
              if (c) { o = { ...o }; } else { o.x = 1; }
              return o;
            }
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn allows_property_assignment_after_a_conditional_copy_issue_8444() {
        // Deliberately still exempt, and the reason is measured rather than
        // argued: #8356 round 4 put full sole ownership here at 6 FP / 0 TP on
        // its corpus. The mutation sits after the whole `if`, so the copy is on
        // one of its paths — unlike the mutually-exclusive case above, which no
        // measurement defends.
        let src = r#"
            export function f(o: Record<string, number>, c: boolean) {
              if (c) { o = { ...o }; }
              o.x = 1;
              return o;
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}

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
    byte_offset_to_line_col, is_constant_index_expression, is_get_context_call_binding,
    is_local_dispatch_table_binding, is_local_object_builder_binding, is_node_module_system_target,
    is_react_display_name_assignment, is_reassigned_fresh_copy_at, is_reduce_accumulator_param,
    is_rtk_reducer_draft_param, is_typed_array_binding, is_unist_visitor_node_param,
    is_valtio_proxy_binding, is_vue_reactive_object_target, is_vue_ref_value_target,
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

/// True when the mutation sits inside a Sentry hook callback — either an inline
/// lambda/method assigned to `beforeSend`/`beforeBreadcrumb`/`beforeSendTransaction`,
/// or a named function registered as one of those hooks somewhere in the file.
/// Sentry's hooks are designed around in-place mutation and offer no immutable API.
fn is_inside_sentry_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // Inline callback: an ancestor object property keyed by a Sentry hook.
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::ObjectProperty(prop) = ancestor.kind()
            && static_key_name(&prop.key).is_some_and(|name| SENTRY_HOOKS.contains(&name))
        {
            return true;
        }
    }

    // Named function registered by reference: `beforeSend: scrubEventRequestUrl`.
    let Some(fn_name) = nearest_enclosing_fn_name(node, semantic) else {
        return false;
    };
    for n in semantic.nodes().iter() {
        if let AstKind::ObjectProperty(prop) = n.kind()
            && static_key_name(&prop.key).is_some_and(|name| SENTRY_HOOKS.contains(&name))
            && let Expression::Identifier(id) = &prop.value
            && id.name.as_str() == fn_name
        {
            return true;
        }
    }
    false
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

/// Get the root `IdentifierReference` from a member-access chain. Used to resolve
/// the binding via semantic and inspect its declaration.
fn root_identifier_of_expr<'a>(expr: &'a Expression<'a>) -> Option<&'a IdentifierReference<'a>> {
    match expr {
        Expression::Identifier(id) => Some(id),
        Expression::StaticMemberExpression(m) => root_identifier_of_expr(&m.object),
        Expression::ComputedMemberExpression(m) => root_identifier_of_expr(&m.object),
        _ => None,
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

/// True when `m` is a constant-keyed indexed write into a freshly-constructed,
/// locally-owned array — `const handlers = []; handlers[0x01] = fn` — i.e. a
/// sparse dispatch/lookup table being built. The base must be a direct
/// identifier resolving to a local empty-array `const` binding, and the index a
/// constant key (numeric literal or `const` opcode). A dynamic index, a foreign
/// or parameter array, or a deeper chain (`obj.table[k]`) does not match, so
/// post-construction or shared-state mutation stays flagged.
fn is_dispatch_table_element_write(
    m: &ComputedMemberExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    matches!(
        &m.object,
        Expression::Identifier(id) if is_local_dispatch_table_binding(id, semantic)
    ) && is_constant_index_expression(&m.expression, semantic)
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
fn is_event_handler_registration(prop_text: &str, value: &Expression) -> bool {
    let is_on_event = prop_text.len() > 2
        && prop_text.starts_with("on")
        && prop_text.as_bytes()[2].is_ascii_lowercase();
    if !is_on_event {
        return false;
    }
    matches!(
        value,
        Expression::ArrowFunctionExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::NullLiteral(_)
    )
}

fn is_create_element_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else { return false };
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

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::AssignmentExpression,
            AstType::UpdateExpression,
            AstType::UnaryExpression,
        ]
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
        match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                // Component.displayName = "Component" (React naming convention)
                if is_react_display_name_assignment(assign) {
                    return;
                }
                // react-three-fiber `useFrame((state) => …)` is the per-frame
                // animation callback; mutating Three.js scene objects in place
                // (`mesh.current.position.y`, `state.camera.position.x`) is the
                // sole supported animation API — Three.js `Vector3`/`Euler`/etc.
                // are stateful instances with no immutable alternative.
                if is_inside_useframe_callback(node, semantic) {
                    return;
                }
                match &assign.left {
                    AssignmentTarget::StaticMemberExpression(m) => {
                        let obj_text = &ctx.source
                            [m.object.span().start as usize..m.object.span().end as usize];
                        let prop_text = m.property.name.as_str();

                        // Vue 3 reactive ref: `count.value = x` drives reactivity.
                        // Also covers a `Ref<T>` destructured from a composable call
                        // (`const { error } = useThing(); error.value = x`).
                        if is_vue_ref_value_target(m, semantic, ctx.project, ctx.path)
                            || crate::oxc_helpers::is_destructured_call_ref_value_target(m, semantic)
                        { return; }
                        // Vue 3 reactive() object: `state.n = x` is the idiomatic update.
                        if is_vue_reactive_object_target(m, semantic, ctx.project, ctx.path) { return; }
                        if obj_text == "module" || obj_text == "exports" { return; }
                        // Node Module-system object: `mod.loaded = true`,
                        // `Module._cache[id] = …` — mutation is the loader contract.
                        if is_node_module_system_target(&m.object, semantic) { return; }
                        if prop_text == "current" { return; }
                        if obj_text == "document" && prop_text == "cookie" { return; }
                        if is_imperative_host_write(obj_text, prop_text) { return; }
                        // `request.onerror = () => …`, `el.onclick = fn` — DOM-style
                        // event-handler registration, not object-state mutation.
                        if is_event_handler_registration(prop_text, &assign.right) { return; }
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
                        // Mutating an object's own instance state (`this.out = sink`)
                        // is encapsulated state, not the external/shared mutation this
                        // rule targets — replacing the whole object is the only
                        // "immutable" form, so there is nothing to suggest.
                        if is_rooted_at_this(&m.object) { return; }
                        if is_inside_sentry_hook(node, semantic) || is_inside_mutation_hook_method(node, semantic) { return; }
                        if root_object_name(&m.object) == Some("set") { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && (is_created_dom_element(id, semantic)
                                || is_local_object_builder_binding(id, semantic)
                                || is_reassigned_fresh_copy_at(id, assign.span.start, semantic)
                                || is_reduce_accumulator_param(id, semantic)
                                || is_rtk_reducer_draft_param(id, semantic)
                                || is_valtio_proxy_binding(id, semantic)
                                || is_get_context_call_binding(id, semantic)
                                || is_unist_visitor_node_param(id, semantic)) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }

                        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation — use spread or immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    AssignmentTarget::ComputedMemberExpression(m) => {
                        let obj_text = &ctx.source
                            [m.object.span().start as usize..m.object.span().end as usize];

                        if obj_text == "module" || obj_text == "exports" { return; }
                        // Node Module-system object: `Module._cache[id] = …`.
                        if is_node_module_system_target(&m.object, semantic) { return; }
                        // TypedArray element write `buf[i] = v`: indexed assignment
                        // is the only way to populate a TypedArray (a fixed-length
                        // binary buffer with no immutable element-setter).
                        if is_typed_array_element_object(&m.object, semantic) { return; }
                        // Sparse dispatch-table construction: `const handlers = [];
                        // handlers[0x01] = fn` builds a locally-owned lookup table by
                        // constant-index assignment — array construction, not mutation.
                        if is_dispatch_table_element_write(m, semantic) { return; }
                        if let Expression::StringLiteral(key) = &m.expression
                            && is_imperative_host_write(obj_text, key.value.as_str()) { return; }
                        // Own instance state: `this.cache[id] = v` — see the static-member arm.
                        if is_rooted_at_this(&m.object) { return; }
                        if is_inside_sentry_hook(node, semantic) || is_inside_mutation_hook_method(node, semantic) { return; }
                        if root_object_name(&m.object) == Some("set") { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && (is_created_dom_element(id, semantic)
                                || is_local_object_builder_binding(id, semantic)
                                || is_reassigned_fresh_copy_at(id, assign.span.start, semantic)
                                || is_reduce_accumulator_param(id, semantic)
                                || is_rtk_reducer_draft_param(id, semantic)
                                || is_valtio_proxy_binding(id, semantic)
                                || is_get_context_call_binding(id, semantic)
                                || is_unist_visitor_node_param(id, semantic)) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }

                        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation — use spread or immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    _ => {}
                }
            }
            AstKind::UpdateExpression(update) => {
                // See the AssignmentExpression arm: in-place Three.js scene-object
                // mutation inside react-three-fiber's `useFrame` callback is the
                // sole supported animation API and has no immutable alternative.
                if is_inside_useframe_callback(node, semantic) {
                    return;
                }
                // update.argument is a SimpleAssignmentTarget.
                // Check if it's a member expression.
                match &update.argument {
                    SimpleAssignmentTarget::StaticMemberExpression(m) => {
                        // Vue 3 reactive ref: `count.value++` drives reactivity.
                        // Also covers a `Ref<T>` destructured from a composable call.
                        if is_vue_ref_value_target(m, semantic, ctx.project, ctx.path)
                            || crate::oxc_helpers::is_destructured_call_ref_value_target(m, semantic)
                        { return; }
                        // Vue 3 reactive() object: `state.incrementedTimes++` is the idiomatic update.
                        if is_vue_reactive_object_target(m, semantic, ctx.project, ctx.path) { return; }
                        // Own instance state: `this.count++` — see the AssignmentExpression arm.
                        if is_rooted_at_this(&m.object) { return; }
                        if is_inside_sentry_hook(node, semantic) || is_inside_mutation_hook_method(node, semantic) { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && (is_created_dom_element(id, semantic)
                                || is_rtk_reducer_draft_param(id, semantic)
                                || is_valtio_proxy_binding(id, semantic)
                                || is_get_context_call_binding(id, semantic)
                                || is_unist_visitor_node_param(id, semantic)) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }
                        let (line, column) = byte_offset_to_line_col(ctx.source, update.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation (increment/decrement) — use immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    SimpleAssignmentTarget::ComputedMemberExpression(m) => {
                        // TypedArray element update `buf[i]++`: same in-place-write idiom.
                        if is_typed_array_element_object(&m.object, semantic) { return; }
                        // Own instance state: `this.counts[k]++` — see the AssignmentExpression arm.
                        if is_rooted_at_this(&m.object) { return; }
                        if is_inside_sentry_hook(node, semantic) || is_inside_mutation_hook_method(node, semantic) { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && (is_created_dom_element(id, semantic)
                                || is_rtk_reducer_draft_param(id, semantic)
                                || is_valtio_proxy_binding(id, semantic)
                                || is_get_context_call_binding(id, semantic)
                                || is_unist_visitor_node_param(id, semantic)) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }
                        let (line, column) = byte_offset_to_line_col(ctx.source, update.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation (increment/decrement) — use immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    _ => {}
                }
            }
            AstKind::UnaryExpression(unary) => {
                if unary.operator != UnaryOperator::Delete {
                    return;
                }
                match &unary.argument {
                    Expression::StaticMemberExpression(m) => {
                        // Own instance state: `delete this.cache.key` — see the AssignmentExpression arm.
                        if is_rooted_at_this(&m.object) { return; }
                        if is_inside_sentry_hook(node, semantic) || is_inside_mutation_hook_method(node, semantic) { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && (is_created_dom_element(id, semantic)
                                || is_local_object_builder_binding(id, semantic)
                                || is_reassigned_fresh_copy_at(id, unary.span.start, semantic)
                                || is_reduce_accumulator_param(id, semantic)
                                || is_rtk_reducer_draft_param(id, semantic)
                                || is_valtio_proxy_binding(id, semantic)
                                || is_get_context_call_binding(id, semantic)
                                || is_unist_visitor_node_param(id, semantic)) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }
                    }
                    Expression::ComputedMemberExpression(m) => {
                        // Own instance state: `delete this.cache[id]` — see the AssignmentExpression arm.
                        if is_rooted_at_this(&m.object) { return; }
                        if is_inside_sentry_hook(node, semantic) || is_inside_mutation_hook_method(node, semantic) { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && (is_created_dom_element(id, semantic)
                                || is_local_object_builder_binding(id, semantic)
                                || is_reassigned_fresh_copy_at(id, unary.span.start, semantic)
                                || is_reduce_accumulator_param(id, semantic)
                                || is_rtk_reducer_draft_param(id, semantic)
                                || is_valtio_proxy_binding(id, semantic)
                                || is_get_context_call_binding(id, semantic)
                                || is_unist_visitor_node_param(id, semantic)) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }
                    }
                    _ => return,
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-property-mutation".into(),
                    message: "Property deletion — use destructuring or immutable patterns.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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
        // flagged. The two `SwitchSink.prototype.x = fn` method-attachment
        // assignments themselves are a distinct pattern outside this issue.
        let src = r#"
            SwitchSink.prototype.event = function(t, stream) {
                this.current = new Segment(t, Infinity, this, this.sink);
                this.current.disposable = stream.source.run(this.current);
            };
            SwitchSink.prototype.end = function(t, x) {
                this.ended = true;
            };
        "#;
        assert_eq!(run(src).len(), 2);
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
            o.value = 5;
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
        let src = r#"
            function reset(el: HTMLElement): void {
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

    #[test]
    fn still_flags_arbitrary_window_property_write_3874() {
        // Negative space: an arbitrary `window.<x>` write is not a documented
        // imperative host API — stashing app state on the global stays flagged.
        let src = r#"
            window.myAppState = { ready: true };
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
        // Only string-literal `displayName` writes are exempt; assigning a
        // computed value is still a property mutation.
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

    // delete on a freshly-spread local object — issue #1336

    #[test]
    fn allows_delete_on_local_object_spread_builder_issue_1336() {
        // Regression for rbaumier/comply#1336 — zod registries: `pm` is a fresh
        // local spread copy; `delete pm.id` omits a key while constructing the
        // returned value, the exact equivalent of the rule's suggested
        // destructuring rest.
        let src = r#"
            function get(p, schema) {
                const pm: any = { ...(this.get(p) ?? {}) };
                delete pm.id;
                return { ...pm, ...this._map.get(schema) };
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_delete_on_local_let_object_spread_builder() {
        let src = r#"
            function build(obj) {
                let copy = { ...obj };
                delete copy.secret;
                return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_delete_on_function_parameter() {
        // A function parameter is external state, not a local object builder.
        let src = r#"
            function f(obj) {
                delete obj.id;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_delete_on_const_from_external_call() {
        // A `const` initialized from a function call (not an object literal /
        // spread) references external state — deleting from it is still flagged.
        let src = r#"
            function f() {
                const x = makeObj();
                delete x.id;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_delete_on_this_member_chain() {
        // Deleting from the object's own instance state is self-state management,
        // not the external/shared mutation this rule targets.
        let src = r#"
            class Foo {
                clear() {
                    delete this.cache.key;
                }
            }
        "#;
        assert!(run(src).is_empty());
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
    fn still_flags_value_mutation_on_plain_const_without_vue_import() {
        // Negative space: `.value` on a const from an external call (not a vue
        // ref factory, no vue import) is a genuine mutation and stays flagged.
        let src = r#"
            const plain = getThing();
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
                     const flag = getThing();\n\
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

    #[test]
    fn still_flags_value_mutation_on_non_destructured_composable_binding() {
        // Negative space: the exemption requires destructuring, so a `.value`
        // write on a plain non-destructured `const x = useThing()` stays flagged.
        let src = r#"
            const x = useThing();
            function f() { x.value = 1; }
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
        // `shallowReactive` follows the same reactive-proxy mutation contract.
        let src = r#"
            import { shallowReactive } from 'vue'
            const state = shallowReactive({ n: 0 });
            state.n = 5;
        "#;
        assert!(run(src).is_empty());
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
    fn still_flags_nested_member_mutation_on_reactive_object() {
        // Negative space: the exemption requires a direct-identifier base, so a
        // nested write `state.inner.n = x` (whose object is itself a member
        // expression) stays flagged.
        let src = r#"
            import { reactive } from 'vue'
            const state = reactive({ inner: { n: 0 } });
            state.inner.n = 1;
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
        // imported from node:module) is an ordinary object — still flagged.
        let src = r#"
            class Module {}
            const mod = new Module(filename);
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
        // Negative space: a plain `Array` element write has immutable
        // alternatives (spread, map) — it stays flagged.
        let src = r#"
            const arr = new Array(3);
            arr[0] = 1;
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
    fn still_flags_dynamic_index_write_on_local_empty_array_5412() {
        // Negative space: a dynamic (non-constant) index is not the dispatch-table
        // signature — `arr[i] = v` with a `let` loop variable stays flagged.
        let src = r#"
            const arr = [];
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
    fn still_flags_parameter_index_write_on_local_empty_array_5412() {
        // Negative space: a parameter index is not a constant key, even when the
        // enclosing function is `const f = (k) => …` — the index must not resolve
        // to the function's own `const` declarator. Runtime indexed write stays
        // flagged.
        let src = r#"
            const handlers = [];
            const register = (k) => {
                handlers[k] = fn;
            };
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
            count.value = 5;
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
        // Strong positive: even after a fresh-copy reassignment, a *later*
        // reassignment to external state (`options = getConfig()`) becomes the
        // nearest preceding write, so the subsequent mutation is still flagged.
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
}

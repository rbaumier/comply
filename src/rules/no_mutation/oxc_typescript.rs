//! no-mutation OXC backend — flag in-place mutating METHOD calls on a
//! `const`-bound value (`arr.push(x)`, `Object.assign(obj, …)`).
//!
//! The assignment axis (`obj.prop = x`, `obj.prop++`) belongs to
//! `no-property-mutation`, which owns `AssignmentExpression` and
//! `UpdateExpression` outright — see the frontier test at the bottom of this
//! file. Subscribing to a kind is what makes a rule speak on it, so not
//! subscribing is how the frontier is enforced rather than asserted.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, expression_is_array, is_call_ref_value_target,
    is_locally_owned_array_binding, is_rtk_reducer_draft_param, is_valtio_proxy_binding,
    is_vue_ref_value_target, root_identifier_of_expr,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey, VariableDeclarationKind};
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
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Test files mutate const restore buffers and `process.env` to inject
        // and reset environment-variable state across cases — the canonical
        // test-time injection surface with no non-mutating alternative.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // Storybook CSF2 attaches story metadata (args, storyName, play,
        // parameters, decorators) by assigning to named properties on the
        // exported story function — the designed API with no immutable
        // alternative; the runner reads these off the function.
        if ctx.file.path_segments.in_storybook {
            return;
        }
        // Sentry's beforeSend/beforeBreadcrumb hooks receive the event by
        // reference, expect in-place mutation, and return the same object —
        // there is no immutable alternative API.
        if is_inside_sentry_hook(node, semantic) {
            return;
        }
        // The only kind this rule subscribes to: `arr.push(x)`,
        // `Object.assign(obj, …)` — the mutating-CALL axis.
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();

        // Object.assign(target, ...)
        if OBJECT_MUTATOR_FUNCTIONS.contains(&method) {
            if let Expression::Identifier(obj) = &member.object
                && obj.name.as_str() == "Object"
                && let Some(first_arg) = call.arguments.first()
            {
                // Skip `Object.assign(fn, { ...literal })` — attaching a
                // static property to a function. JS has no immutable
                // alternative; see rbaumier/comply#154.
                if method == "assign" && is_assign_static_to_function(call, semantic) {
                    return;
                }
                let root = match first_arg.as_expression() {
                    Some(Expression::Identifier(ident)) => Some(ident.name.as_str()),
                    Some(expr) => root_name_of_expr(expr),
                    None => None,
                };
                if let Some(root) = root && is_declared_as_const(semantic, root) {
                    report(diagnostics, ctx, call.span.start, root, "Mutating");
                }
            }
            return;
        }

        if !MUTATING_ARRAY_METHODS.contains(&method) {
            return;
        }

        // `state.ids.push(…)` mutates an intentional-mutation target: a
        // Redux Toolkit reducer's Immer draft (the documented RTK pattern,
        // not aliased state) or a valtio `proxy()` binding (direct mutation
        // is valtio's entire API).
        if let Some(id) = root_identifier_of_expr(&member.object)
            && (is_rtk_reducer_draft_param(id, semantic)
                || is_valtio_proxy_binding(id, semantic))
        {
            return;
        }

        // Vue 3 reactive ref: `list.value.push(x)` / `list.value.splice(…)`
        // mutates the deeply-reactive array a `ref([])` holds — the
        // idiomatic, referentially-stable update, with no immutable
        // alternative (`list.value = [...list.value, x]` reallocates and
        // drops the array's reactive identity). The receiver `<ref>.value`
        // is itself a `.value` member access whose base is the ref
        // identifier, so `member.object` is the `<ref>.value` node passed to
        // both predicates. `is_call_ref_value_target` covers the ref a
        // composable returned (`const items = useLocalStorage(k, [])`), whose
        // factory is not one of Vue's own — the same disjunction
        // `no-property-mutation` applies to `items.value = x`.
        if let Expression::StaticMemberExpression(inner) = &member.object
            && (is_vue_ref_value_target(inner, semantic, ctx.project, ctx.path)
                || is_call_ref_value_target(inner, semantic))
        {
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

        // Skip ANY mutating array method on a locally-owned fresh array —
        // a `VariableDeclarator` array-literal (or `new Array(...)`)
        // binding in a non-module scope — regardless of loop context.
        // Nothing outside the declaring function observes the mutation
        // (the "build a local array, then return/consume it" pattern), so
        // it is not the shared-state mutation this rule targets. The method
        // name carries no information here: `out.sort()` on a fresh local is
        // exactly as unobservable as `out.push(x)`, which is why the sibling
        // `no-mutating-methods` exempts the whole set on the same predicate.
        // A parameter, module-scope, or member-expression receiver is not
        // locally owned and stays flagged.
        if let Expression::Identifier(receiver) = &member.object
            && is_locally_owned_array_binding(receiver, semantic)
        {
            return;
        }

        // An array-mutation method on a plain-identifier receiver is an
        // array mutation only with positive evidence that the binding is
        // array-shaped: an array-literal / `new Array(...)` / array-returning
        // initializer, or an explicit array type annotation (`T[]`,
        // `readonly T[]`, `Array<T>`, `ReadonlyArray<T>`) on its declarator or
        // parameter. A receiver bound to a non-array factory
        // (`const router = useRouter()`, `const s = new Subject()`) carries no
        // such evidence and calls a same-named method on a different object
        // (navigation, an event emitter), not an array. Member receivers
        // (`store.items`, `this.children`) are not gated — their element type
        // is not locally resolvable — so they stay flagged.
        if matches!(&member.object, Expression::Identifier(_))
            && !expression_is_array(&member.object, semantic)
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
}

const SENTRY_HOOKS: &[&str] = &["beforeSend", "beforeBreadcrumb", "beforeSendTransaction"];

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
fn is_inside_sentry_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::ObjectProperty(prop) = ancestor.kind()
            && static_key_name(&prop.key).is_some_and(|name| SENTRY_HOOKS.contains(&name))
        {
            return true;
        }
    }

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

/// True when `call` is `Object.assign(fn, { ...literal })` where `fn` is
/// an identifier bound to a `const`-declared function/arrow expression.
/// Recognises the JS-canonical "attach static prop to a function" pattern.
fn is_assign_static_to_function(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(first) = call.arguments.first() else { return false };
    let Some(second) = call.arguments.get(1) else { return false };

    if !matches!(second, oxc_ast::ast::Argument::ObjectExpression(_)) {
        return false;
    }

    let oxc_ast::ast::Argument::Identifier(ident) = first else { return false };
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        if let AstKind::VariableDeclarator(decl) = kind {
            return matches!(
                decl.init,
                Some(Expression::ArrowFunctionExpression(_))
                    | Some(Expression::FunctionExpression(_)),
            );
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
    matches!(obj.name.as_str(), "Result" | "Effect") && member.property.name.as_str() == "gen"
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

    #[test]
    fn ignores_typed_accumulator_two_step_yield_in_result_gen() {
        // Regression for rbaumier/comply#363 — exact amadeo pattern:
        // type-annotated const, two-step (separate yield + push), Result.ok wrapper.
        let src = r#"
            type User = { id: string };
            function getUsers(rows: unknown[], orgId: string) {
                return Result.gen(function* () {
                    const items: User[] = [];
                    for (const row of rows) {
                        const user = yield* rowToUser(row as any, orgId);
                        items.push(user);
                    }
                    return Result.ok({ items, total: items.length });
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_effect_gen_without_loop() {
        // Effect.gen (effect-ts) uses the same sequential-yield accumulator
        // pattern and must be treated the same as Result.gen.
        let src = r#"
            type User = { id: string };
            function fetchTwo() {
                return Effect.gen(function* () {
                    const users: User[] = [];
                    const u1 = yield* fetchUser("id1");
                    users.push(u1);
                    const u2 = yield* fetchUser("id2");
                    users.push(u2);
                    return users;
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_object_assign_attaching_static_to_function() {
        // Regression for rbaumier/comply#154 — Object.assign on a function
        // const with an object literal is the canonical static-prop pattern.
        let src = r#"
            const defaults = { mode: "strict" };
            const parser = (input: unknown) => input;
            return Object.assign(parser, { defaults });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_object_assign_on_plain_const() {
        let src = r#"
            const target = { a: 1 };
            Object.assign(target, { b: 2 });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Sentry beforeSend/beforeBreadcrumb in-place scrub hooks — issue #478

    #[test]
    fn allows_const_mutation_inside_inline_before_breadcrumb_method() {
        let src = r#"
            Sentry.init({
                beforeBreadcrumb(breadcrumb) {
                    const trail: unknown[] = breadcrumb.trail;
                    trail.push(scrubSensitiveQueryFromUrl(breadcrumb.url));
                    return breadcrumb;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_const_mutation_in_named_function_registered_as_before_send() {
        let src = r#"
            function scrubEvent(event) {
                const frames: unknown[] = event.frames;
                frames.sort();
                return event;
            }
            Sentry.init({ beforeSend: scrubEvent });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_const_mutation_outside_sentry_hook() {
        // Negative space: the same call outside any registered hook is an
        // ordinary mutation of a value the function did not build.
        let src = r#"
            function scrub() {
                const frames: unknown[] = getFrames();
                frames.sort();
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Storybook CSF2 story-file exemption — issue #1680

    fn storybook_file_ctx() -> crate::rules::file_ctx::FileCtx {
        crate::rules::file_ctx::FileCtx {
            path_segments: crate::rules::file_ctx::PathSegments {
                in_storybook: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn allows_mutating_call_in_a_story_file_issue_1680() {
        // A CSF2 story file configures its exported story function in place —
        // the designed API the runner reads back, with no immutable form. The
        // guard is on the file, so it covers the call axis this rule keeps.
        let src = "const Story = () => null; Object.assign(Story, { args: {} });";
        let file = storybook_file_ctx();
        let diagnostics = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            "Button.stories.tsx",
            crate::project::default_static_project_ctx(),
            &file,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn leaves_the_delete_operator_to_no_delete_issue_8356() {
        // Boundary for rbaumier/comply#8356 — one operator, one rule: `no-delete`
        // decides every `delete`, including on a `const`-bound receiver. Not
        // subscribing to the kind is what makes that true; the engine never
        // dispatches it here.
        assert!(!Check.interested_kinds().contains(&AstType::UnaryExpression));
    }

    // Redux Toolkit Immer draft mutations — issue #5596

    #[test]
    fn allows_draft_array_push_in_create_slice_reducer_issue_5596() {
        // A mutating array method (`state.ids.push(…)`) on the Immer draft inside
        // a createSlice reducer is the documented RTK pattern, not aliased state.
        let src = r#"
            import { createSlice } from '@reduxjs/toolkit'
            const slice = createSlice({
                name: 'entities',
                initialState,
                reducers: {
                    addOne(state, action) {
                        state.ids.push(action.payload.id);
                    },
                },
            })
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_draft_array_push_in_create_reducer_add_case_issue_5596() {
        // Same draft array mutation through `builder.addCase`'s case reducer.
        let src = r#"
            import { createReducer } from '@reduxjs/toolkit'
            const reducer = createReducer(initialState, (builder) => {
                builder.addCase(addTodo, (state, action) => {
                    state.todos.push(action.payload);
                });
            })
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_draft_typed_array_push_issue_5596() {
        // A `Draft<…>`-typed state parameter mutated via a helper — the entity
        // adapter shape; the `Draft` annotation is the structural signal.
        let src = r#"
            import type { Draft } from 'immer';
            function addOneMutably(entity, state: Draft<R>) {
                state.ids.push(entity.id);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_ordinary_const_array_push_outside_reducer_issue_5596() {
        // Negative space: `.push` on a plain const array (positive array evidence
        // via the `number[]` annotation) outside any reducer (no RTK context, no
        // `Draft<…>` type) stays flagged.
        let src = r#"
            function f() {
                const list: number[] = getList();
                list.push(1);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_immer_draft_typed_const_push_issue_5596() {
        // Negative space: a `Draft<…>` annotation not imported from `immer` is a
        // same-named domain type — `.push` on it stays flagged.
        let src = r#"
            type Draft<T> = T;
            function f() {
                const doc: Draft<Doc> = getDoc();
                doc.items.push(1);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_non_draft_const_mutation_inside_reducer_issue_5596() {
        // Negative space: a captured outer `const` array (positive array evidence
        // via the `unknown[]` annotation) mutated inside a reducer is not the draft
        // (not the reducer's first param) — it stays flagged.
        let src = r#"
            import { createSlice } from '@reduxjs/toolkit'
            const cache: unknown[] = getCache();
            const slice = createSlice({
                name: 's',
                initialState,
                reducers: {
                    update(state, action) {
                        cache.push(action.payload);
                    },
                },
            })
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // valtio proxy() reactive mutations — issue #5595

    #[test]
    fn allows_valtio_proxy_mutations_issue_5595() {
        // Regression for rbaumier/comply#5595 — valtio's `proxy()` returns a
        // reactive Proxy whose direct mutation IS the API: a mutating array
        // method on a `const` proxy binding drives reactivity, with no immutable
        // alternative.
        let src = r#"
            import { proxy } from 'valtio'
            const state = proxy({ number: 0, nested: { ticks: 0 }, items: [] })
            state.items.push(1)
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_plain_const_mutation_not_valtio_proxy() {
        // Negative space: a plain `const` array (not initialised by `proxy()`
        // from valtio) is not a reactive proxy — mutating it stays flagged, even
        // in a file that imports `proxy` from valtio.
        let src = r#"
            import { proxy } from 'valtio'
            const state = proxy({ n: 0 });
            const plain: unknown[] = getItems();
            plain.push(1);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_local_proxy_not_imported_from_valtio() {
        // Negative space: a same-named local `proxy()` (not imported from valtio)
        // returns a plain value — a mutating method on it stays flagged.
        let src = r#"
            function proxy(x) { return x; }
            const state: unknown[] = proxy([]);
            state.push(1);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Locally-owned fresh-array accumulator, conditional push outside a loop —
    // issue #7593

    #[test]
    fn ignores_conditional_push_on_local_array_literal_issue_7593() {
        // Regression for rbaumier/comply#7593 — documenso search filters: a fresh
        // function-scope array literal built up with a conditional push, then
        // consumed locally (Prisma `where`). Not observable outside the function.
        let src = r#"
            function searchDocuments(user, teamIds, query) {
                const filters = [
                    { recipients: { some: { email: user.email } }, title: { contains: query } },
                ];
                if (teamIds.length > 0) {
                    filters.push({ teamId: { in: teamIds } });
                }
                return prisma.document.findMany({ where: { OR: filters } });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_conditional_push_on_typed_empty_local_array_issue_7593() {
        // Regression for rbaumier/comply#7593 — documenso audit logs: a typed
        // empty local array (`const auditLogs: T[] = []`) accumulated via
        // conditional pushes, then returned.
        let src = r#"
            type AuditLog = { type: string };
            function updateEnvelope(isTitleSame, isExternalIdSame) {
                const auditLogs: AuditLog[] = [];
                if (!isTitleSame) {
                    auditLogs.push({ type: "title" });
                }
                if (!isExternalIdSame) {
                    auditLogs.push({ type: "externalId" });
                }
                return auditLogs;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_top_of_function_push_on_local_array_issue_7593() {
        // Regression for rbaumier/comply#7593 — documenso breadcrumbs: a fresh
        // local array pushed at the top of the function body (no loop). The
        // receiver is a locally-owned identifier, so it is exempt.
        let src = r#"
            function getFolderBreadcrumbs(currentFolder) {
                const breadcrumbs = [];
                breadcrumbs.push(currentFolder);
                return breadcrumbs;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_push_on_module_scope_const_array_issue_7593() {
        // Negative space: a module-scope array is reachable by other code in the
        // module, so its mutation stays observable and flagged.
        let src = r#"
            const registry: number[] = [];
            export function register(x: number) {
                registry.push(x);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_push_on_member_property_array_issue_7593() {
        // Negative space: `store.items.push(x)` mutates shared object state — the
        // receiver is a member access, not a plain local identifier, so the
        // locally-owned exemption does not apply.
        let src = r#"
            function add(x) {
                const store = getStore();
                store.items.push(x);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Array-mutation method names collide with non-array `.push()` APIs
    // (vue-router `Router`, history, RxJS `Subject`, custom `Stack`) — issue #7732

    #[test]
    fn ignores_push_on_vue_router_instance_issue_7732() {
        // Regression for rbaumier/comply#7732 — `useRouter()` returns a vue-router
        // `Router`; `router.push(path)` navigates and has no immutable alternative.
        // The receiver carries no array evidence, so the array-method name alone
        // must not drive the diagnostic.
        let src = r#"
            const router = useRouter();
            function go() {
                router.push("/");
                router.push({ name: "home", query: { q: "1" } });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_on_history_and_subject_and_opaque_call_issue_7732() {
        // Other non-array receivers with a `.push()` method: a history object, an
        // RxJS `Subject` (`new Subject()`), and a const bound to any opaque factory
        // call. None carries array evidence, so none is flagged — proving the gate
        // keys on structural array evidence, not a receiver-name allowlist.
        let src = r#"
            const h = createHistory();
            const s = new Subject();
            const list = getList();
            function run() {
                h.push("/next");
                s.push(1);
                list.push(1);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_array_literal_and_producer_and_annotation_receivers_issue_7732() {
        // True positives preserved: a plain-identifier receiver with positive array
        // evidence — an array literal, an array-producing chain (`.split()`,
        // `Array.from(...)`), or an explicit `T[]` annotation — stays flagged.
        let array_literal = r#"
            const xs = [];
            xs.push(1);
        "#;
        assert_eq!(run(array_literal).len(), 1);
        let annotation = r#"
            const xs: number[] = f();
            xs.splice(0, 1);
        "#;
        assert_eq!(run(annotation).len(), 1);
        let split_producer = r#"
            const xs = "a,b".split(",");
            xs.push("c");
        "#;
        assert_eq!(run(split_producer).len(), 1);
        let array_from_producer = r#"
            const xs = Array.from(it);
            xs.unshift(0);
        "#;
        assert_eq!(run(array_from_producer).len(), 1);
    }

    // Vue 3 reactive ref `.value` array mutation — issue #7777

    #[test]
    fn allows_mutating_array_method_on_vue_ref_value_issue_7777() {
        // Regression for rbaumier/comply#7777 — `const list = ref([])` holds a
        // deeply-reactive array; `list.value.push(x)` / `list.value.splice(…)` is
        // the idiomatic in-place update. Reassigning `list.value` (also exempt)
        // reallocates and drops the array's reactive identity.
        let src = r#"
            import { ref } from 'vue'
            const tabsList = ref([]);
            function update(route, index, targetIndex) {
                tabsList.value.push(route);
                tabsList.value.splice(0, index);
                const current = tabsList.value.splice(targetIndex, 1);
                tabsList.value = current;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutating_array_method_on_shallow_ref_value_issue_7777() {
        // `shallowRef` is a writable Vue ref factory too; a mutating array method
        // on its `.value` array takes the same exemption.
        let src = r#"
            import { shallowRef } from 'vue'
            const items = shallowRef([]);
            function drop() {
                items.value.pop();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_mutating_array_method_on_non_ref_value_object_issue_7777() {
        // Negative space: the exemption is gated on `is_vue_ref_value_target`, so a
        // plain object with a `value` array field (not a Vue ref) stays flagged —
        // `.value` alone is not the reactive signal.
        let src = r#"
            const o = { value: [] };
            o.value.push(1);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Frontier with no-property-mutation — issue #8441

    #[test]
    fn leaves_the_assignment_axis_to_no_property_mutation_issue_8441() {
        // Boundary for rbaumier/comply#8441 — one position, one rule. Property
        // WRITES (`o.p = x`, `o.p++`) belong to `no-property-mutation`, which
        // subscribes to both kinds and carries every exemption for them; this
        // rule keeps the CALL axis. Not subscribing is what enforces the
        // partition — the engine never dispatches those kinds here — so the
        // assertion is on the subscription, not on a diagnostic count that a
        // later exemption could make pass for the wrong reason.
        assert!(!Check.interested_kinds().contains(&AstType::AssignmentExpression));
        assert!(!Check.interested_kinds().contains(&AstType::UpdateExpression));
    }

    #[test]
    fn stays_silent_on_a_property_write_it_no_longer_owns_issue_8441() {
        // The other half of the boundary: a `const`-bound receiver whose property
        // is assigned and incremented — the exact shape this rule used to double
        // report — draws nothing here, while the mutating call on the same
        // binding still does.
        let src = r#"
            const target = getTarget();
            target.count = 1;
            target.count++;
        "#;
        assert!(run(src).is_empty());
        assert_eq!(run("const target = getTarget(); target.items.push(1);").len(), 1);
    }

    // Composable-returned ref holding an array — issue #7849

    #[test]
    fn allows_mutating_array_method_on_composable_ref_value_issue_7849() {
        // Regression for rbaumier/comply#7849: `useLocalStorage` is not in
        // `VUE_REF_FACTORIES`, so only `is_call_ref_value_target` recognises the
        // `Ref<T[]>` it returns. Reassigning `items.value` was already exempt;
        // mutating it in place was not, for the same binding.
        let src = r#"
            const items = useLocalStorage('k', []);
            items.value.push(1);
        "#;
        assert!(run(src).is_empty());
    }

    // Locally-owned array, every mutating method — issue #7661

    #[test]
    fn allows_sort_on_a_locally_owned_fresh_array_issue_7661() {
        // Regression for rbaumier/comply#7661: the locally-owned-array exemption
        // was gated on `push`/`unshift`, so the sibling rule
        // `no-mutating-methods` stayed silent on this very code while this one
        // reported it. Ownership does not depend on which method reorders the
        // array.
        let src = r#"
            function ordered(items) {
                const out = [...items];
                out.sort();
                return out;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_sort_on_a_module_scope_array_issue_7661() {
        // Negative space: dropping the method gate did not drop the ownership
        // gate — a module-scope array is reachable by every importer, so
        // reordering it stays flagged whichever method does it.
        let src = r#"
            const cache = [];
            cache.sort();
        "#;
        assert_eq!(run(src).len(), 1);
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
        severity: Severity::Error,
        span: None,
    });
}

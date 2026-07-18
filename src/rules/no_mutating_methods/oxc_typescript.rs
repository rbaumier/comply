use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, expression_is_array, is_array_initializer,
    is_node_module_system_target, is_reduce_accumulator_param, is_vue_ref_value_target,
    locally_owned_binding_init, resolves_to_import_from,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const MUTATING: &[&str] = &[
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let name = member.property.name.as_str();
        if !MUTATING.contains(&name) {
            return;
        }
        // `super.push(...)` inside an `override async push(...)` dispatches to the
        // superclass's overriding method, never `Array.prototype.push`; a `super`
        // receiver is structurally never an array.
        if matches!(&member.object, Expression::Super(_)) {
            return;
        }
        if name == "fill" && is_non_array_fill(member, call, ctx, semantic) {
            return;
        }
        if name == "push" && is_router_navigation(member, semantic) {
            return;
        }
        // Node Module-system object: `ctx.parentModule.children.push(mod)` mutates
        // the Node-owned CJS dependency-graph array — the module-loader contract.
        if is_node_module_system_target(&member.object, semantic) {
            return;
        }

        // Vue 3 reactive ref: `tabs.value.push(x)` / `tabs.value.splice(...)`
        // mutates the deeply-reactive array a `ref([])` holds — the idiomatic,
        // referentially-stable Vue 3 update. The receiver `<ref>.value` is itself
        // a member access whose base is the ref identifier, so the receiver's
        // `.object` is the `<ref>.value` node passed to `is_vue_ref_value_target`.
        // The non-mutating alternative (`tabs.value = [...tabs.value, x]`)
        // reallocates the array and drops its reactive identity. Parity with
        // `no-mutation` / `no-property-mutation`, which exempt `<ref>.value` writes.
        if let Expression::StaticMemberExpression(inner) = &member.object
            && is_vue_ref_value_target(inner, semantic, ctx.project, ctx.path)
        {
            return;
        }

        // Pinia option-store action: `this.<state>.push(x)` /
        // `this.<state>.splice(...)` mutates the Vue reactive proxy that Pinia
        // builds from the store's `state`. Inside an `actions` method of a
        // `defineStore(id, { ... })` call (with `defineStore` imported from
        // `pinia`), mutating a `this.<state>` array in place is the idiomatic,
        // referentially-stable Pinia update — the same reactivity mechanism as
        // the `<ref>.value` case above (`reactive()` under the hood), with no
        // immutable alternative that preserves the proxy's identity.
        if is_pinia_store_action_this_mutation(member, node, semantic) {
            return;
        }

        // Reduce accumulator: `menus.reduce((acc, cur) => { acc.push(cur); return
        // acc; }, [])` mutates the callback's accumulator parameter — the canonical
        // reduce pattern, where the accumulator is threaded through and returned,
        // never observed elsewhere until `reduce` completes. Parity with
        // `no-mutation` / `no-property-mutation`, which exempt reduce-accumulator
        // mutations via the same helper.
        if let Expression::Identifier(receiver) = &member.object
            && is_reduce_accumulator_param(receiver, semantic)
        {
            return;
        }

        // Mutation of a locally-owned array is not externally observable: the
        // receiver resolves to a `const`/`let` declared in an inner (non-module)
        // scope whose initialiser is an array-evident expression — an array
        // literal (`[...]`, `[]`), a `new Array(...)`, or a fresh-array-producing
        // call (`data.map(...)`, `items.filter(...)`, `str.split(...)`,
        // `Array.from(...)`, `Object.keys(...)`). This covers both the
        // "build up an accumulator then return it" pattern (`const actions =
        // [a, b]; actions.push(c); return v.pipe(...actions)`) and the
        // "fetch `perPage + 1`, then `.pop()` the extra" pagination trick
        // (`const rows = data.map(...); const extra = rows.pop()`). A parameter
        // array, a module-scope array, or a property (`this.items`, `obj.list`)
        // is not exempt — those may be observed by other code.
        if let Expression::Identifier(receiver) = &member.object
            && locally_owned_binding_init(receiver, semantic)
                .is_some_and(is_array_evident_initializer)
        {
            return;
        }

        // Mutation of a fresh array with no pre-existing reference is not
        // externally observable — the receiver expression itself allocates the
        // array, so nothing else can hold a reference. Qualifying shapes:
        // - an inline array literal (`[...referencedColumns].sort(...)`,
        //   `[a, b].reverse()`, `[...x].splice(0, 1)`);
        // - a `new Array(n)` construction (`new Array(n).fill(0)`);
        // - an array-returning call: a chained producer
        //   (`children.slice(0, n).reverse()`, `items.filter(...).sort(...)`)
        //   or a static one (`Object.keys(o).sort()`, `Array.from(x).sort()`).
        // In every case nothing else references the array — the canonical
        // "sort/reverse/fill a fresh array" idiom. A receiver whose call is not
        // a fresh-array producer (`obj.getList().reverse()`) may return a shared
        // array, so it stays flagged; a plain-identifier or member receiver is
        // never array-evident here and is handled by the binding checks
        // above/below.
        if is_array_evident_initializer(&member.object) {
            return;
        }

        // Bounded local accumulator inside a `for` / `for-of` / `for-in`
        // loop: `const items = []; for (...) items.push(yield* fn());`.
        // The non-mutating spread alternative is O(n²) and the
        // canonical functional alternative (`Result.all(rows.map(...))`)
        // does not exist in better-result yet — tracking upstream at
        // https://github.com/dmmulroy/better-result/issues/32. Once
        // that lands, callers can switch to `Result.all` and this skip
        // becomes unnecessary.
        //
        // Same exemption inside a `Result.gen(function*() { ... })`
        // block — the generator body is the canonical accumulator site
        // for sequencing `yield*` results into a local array, and the
        // spread alternative breaks short-circuiting on the first
        // error.
        if matches!(name, "push" | "unshift")
            && matches!(&member.object, Expression::Identifier(_))
            && (is_inside_loop_body(node, semantic) || is_inside_result_gen(node, semantic))
        {
            return;
        }

        // A plain-identifier receiver is an `Array.prototype` mutation only with
        // positive evidence that the binding is array-typed: an array-literal /
        // `new Array(...)` / array-returning initialiser, or an explicit array
        // type annotation (`T[]`, `readonly T[]`, `Array<T>`, `ReadonlyArray<T>`)
        // on its declarator or parameter. A receiver bound to a non-array factory
        // (`const adapter = this.selectAdapter(...)`) carries no such evidence and
        // calls a same-named method on a custom object, not an array append.
        // Member receivers (`this.items`, `node.children`) are not gated here —
        // their element type is not locally resolvable — so they stay flagged.
        if matches!(&member.object, Expression::Identifier(_))
            && !expression_is_array(&member.object, semantic)
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{name}()` mutates the array in place \u{2014} use a non-mutating alternative (spread, `slice`, `toSorted`, `toReversed`, `toSpliced`, `filter`, `map`, `concat`)."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when a `.fill(...)` call is not an `Array.prototype.fill` mutation.
///
/// `Array.prototype.fill(value, start?, end?)` always passes a fill value, so
/// distinct same-named methods are recognised by shape rather than by a
/// receiver-name allowlist:
/// - a zero-argument `.fill()` is the Canvas2D `context.fill()` drawing call
///   (`Array.prototype.fill()` with no value is degenerate and not written);
/// - a chained receiver (`page.getByLabel(...).fill(...)`, `this.input.fill(...)`)
///   is a Playwright/Locator interaction, not an array literal;
/// - any `.fill(...)` inside a test/spec file is a Playwright/Cypress locator
///   fill (`label.fill(text)`), where the receiver type cannot be recovered
///   without type information;
/// - a bare-identifier receiver (`field.fill("12")`) is treated as
///   `Array.prototype.fill` only when the binding is array-evident — its
///   initialiser is an array literal, `new Array(...)`, or an array-returning
///   call (`.map`/`.filter`/`.slice`/`.concat`/`Array.from`). A receiver bound
///   to an arbitrary member-chain such as a Playwright `Locator`
///   (`const field = page.getByLabel(...)`) is not array-evident, so the
///   single-argument `field.fill(value)` is a Locator interaction.
fn is_non_array_fill(
    member: &oxc_ast::ast::StaticMemberExpression,
    call: &oxc_ast::ast::CallExpression,
    ctx: &CheckCtx,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    if call.arguments.is_empty()
        || matches!(
            &member.object,
            Expression::CallExpression(_)
                | Expression::StaticMemberExpression(_)
                | Expression::ComputedMemberExpression(_)
        )
        || ctx.file.path_segments.in_test_dir
    {
        return true;
    }
    // A single-argument `.fill(value)` on a bare identifier is an
    // `Array.prototype.fill` mutation only when the binding is array-evident;
    // otherwise the receiver is an opaque value (e.g. a Playwright `Locator`).
    if let Expression::Identifier(receiver) = &member.object {
        return !receiver_initializer(receiver, semantic).is_some_and(is_array_evident_initializer);
    }
    false
}

/// `true` when `expr` proves its value is a fresh array: an array literal
/// (`[...]`, `[]`), a `new Array(...)` construction, an array-returning call
/// (`x.map(...)`, `x.filter(...)`, `x.slice(...)`, `x.concat(...)`,
/// `str.split(...)`, `Array.from(...)`, `Array.of(...)`), or an
/// `Object.keys`/`Object.values`/`Object.entries(...)` static producer.
///
/// `slice`/`concat`/`split` also exist on `String`, so this accepts the same
/// receiver-type imprecision the rule already carries for `slice`/`concat`:
/// `String.prototype.split` always returns a fresh `string[]`.
fn is_array_evident_initializer(expr: &Expression) -> bool {
    if is_array_initializer(expr) {
        return true;
    }
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    match &call.callee {
        Expression::StaticMemberExpression(member) => {
            let method = member.property.name.as_str();
            if let Expression::Identifier(id) = &member.object {
                match id.name.as_str() {
                    // `Array.from(...)` / `Array.of(...)` build a fresh array.
                    "Array" => return matches!(method, "from" | "of"),
                    // `Object.keys`/`values`/`entries(...)` each return a fresh
                    // array (`string[]` / `unknown[]` / `[string, unknown][]`).
                    "Object" => return matches!(method, "keys" | "values" | "entries"),
                    _ => {}
                }
            }
            matches!(
                method,
                "map" | "filter" | "slice" | "concat" | "split" | "flat" | "flatMap" | "toSorted"
                    | "toReversed" | "toSpliced" | "fill"
            )
        }
        _ => false,
    }
}

/// True when a `.push(...)` call is a router/history navigation, not an
/// `Array.prototype.push` mutation.
///
/// `router.push(url)` (Next.js `useRouter`, Vue Router) and `history.push(url)`
/// (React Router v5, Reach Router) navigate to a URL; they share the name
/// `push` with `Array.prototype.push` but are unrelated. The receiver is
/// recognised by two complementary signals:
/// - binding shape: the receiver identifier resolves to a `const`/`let` whose
///   initialiser is a navigation factory (`useRouter()`, `useNavigate()`,
///   `useHistory()`, `createBrowserHistory()`, …) — this holds regardless of
///   the variable's name;
/// - name convention: the receiver is named `router`, `history`, `navigate`, or
///   `navigation`, covering receivers that cannot be traced to a factory
///   (function parameters, props, destructured values).
fn is_router_navigation(
    member: &oxc_ast::ast::StaticMemberExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Expression::Identifier(receiver) = &member.object else {
        return false;
    };
    if is_navigation_identifier(receiver.name.as_str()) {
        return true;
    }
    receiver_initializer(receiver, semantic).is_some_and(is_navigation_factory_call)
}

/// `true` for the conventional names given to router/history objects.
fn is_navigation_identifier(name: &str) -> bool {
    matches!(name, "router" | "history" | "navigate" | "navigation")
}

/// Resolve a receiver identifier to the initialiser of its declaring
/// `const`/`let`, when it is declared by a `VariableDeclarator` in scope.
fn receiver_initializer<'a>(
    receiver: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a Expression<'a>> {
    let scoping = semantic.scoping();
    let ref_id = receiver.reference_id.get()?;
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    std::iter::once(nodes.kind(decl_id))
        .chain(nodes.ancestor_kinds(decl_id))
        .find_map(|kind| match kind {
            AstKind::VariableDeclarator(decl) => Some(decl.init.as_ref()),
            _ => None,
        })
        .flatten()
}

/// `true` when `expr` is a call to a router/history factory hook, e.g.
/// `useRouter()`, `useNavigate()`, `createBrowserHistory()`.
fn is_navigation_factory_call(expr: &Expression) -> bool {
    let callee_name = match expr {
        Expression::CallExpression(call) => match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(member) => member.property.name.as_str(),
            _ => return false,
        },
        _ => return false,
    };
    matches!(
        callee_name,
        "useRouter"
            | "useNavigate"
            | "useHistory"
            | "createBrowserHistory"
            | "createHashHistory"
            | "createMemoryHistory"
    )
}

/// True when the mutating call is a `this.<state>` array mutation inside the
/// `actions` object of a Pinia `defineStore(id, { ... })` option store.
///
/// Recognised by structural call-site/ancestor provenance, not a receiver name:
/// 1. the call's receiver is `this.<member>` — a `StaticMemberExpression` whose
///    base is a `ThisExpression` (`this.list`, single level);
/// 2. the nearest enclosing function is the value of an object property that
///    belongs to the object literal held by an `actions` property;
/// 3. that `actions` property's object literal is the second (options) argument
///    of a `defineStore(...)` call whose `defineStore` callee resolves to an
///    import from `pinia`.
///
/// In a Pinia option store `this.<state>` is a `reactive()` proxy, so mutating
/// the array it holds in place is the prescribed reactive update — the same
/// mechanism the `<ref>.value` / `reactive(...)` exemptions already cover. A
/// plain `class Store { m() { this.items.push(x) } }`, a `getters` method, or a
/// `defineStore` not imported from `pinia` fails one of the three gates and
/// stays flagged.
fn is_pinia_store_action_this_mutation(
    member: &oxc_ast::ast::StaticMemberExpression,
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::{Expression, PropertyKey};
    use oxc_span::GetSpan;

    // (1) receiver is `this.<member>`.
    let Expression::StaticMemberExpression(receiver) = &member.object else {
        return false;
    };
    if !matches!(&receiver.object, Expression::ThisExpression(_)) {
        return false;
    }

    let nodes = semantic.nodes();

    // (2) nearest enclosing function = the action method's function.
    let Some(func) = nodes.ancestors(node.id()).find(|ancestor| {
        matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        )
    }) else {
        return false;
    };
    // …held as the value of an object property (the action entry)…
    let action_prop = nodes.parent_node(func.id());
    if !matches!(action_prop.kind(), AstKind::ObjectProperty(_)) {
        return false;
    }
    // …in an object literal (the `actions` object)…
    let actions_obj = nodes.parent_node(action_prop.id());
    if !matches!(actions_obj.kind(), AstKind::ObjectExpression(_)) {
        return false;
    }
    // …that is the value of a property keyed `actions`.
    let actions_container = nodes.parent_node(actions_obj.id());
    let AstKind::ObjectProperty(container) = actions_container.kind() else {
        return false;
    };
    let keyed_actions = match &container.key {
        PropertyKey::StaticIdentifier(id) => id.name == "actions",
        PropertyKey::StringLiteral(s) => s.value == "actions",
        _ => false,
    };
    if !keyed_actions {
        return false;
    }

    // (3) the options object literal is the second argument of a
    // `defineStore(...)` call whose callee is imported from `pinia`.
    let options_node = nodes.parent_node(actions_container.id());
    let AstKind::ObjectExpression(options) = options_node.kind() else {
        return false;
    };
    let AstKind::CallExpression(call) = nodes.parent_node(options_node.id()).kind() else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    if callee.name != "defineStore" || !resolves_to_import_from(callee, semantic, &["pinia"]) {
        return false;
    }
    call.arguments
        .get(1)
        .and_then(oxc_ast::ast::Argument::as_expression)
        .is_some_and(|arg| arg.span() == options.span)
}

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
            // Stop at function boundary — pushes inside a callback
            // passed to a sibling helper are not "this function's
            // accumulator".
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
                // The generator must be the direct argument of a
                // `Result.gen(...)` call.
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

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, src, path)
    }

    #[test]
    fn ignores_zero_arg_canvas_fill() {
        // Regression for rbaumier/comply#1688 — CanvasRenderingContext2D.fill()
        // takes no fill value, so it is never an Array.prototype.fill mutation.
        let src = r#"
            function drawLabel(context) {
                context.fillStyle = "red";
                context.fill();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_playwright_locator_fill_in_spec_file() {
        // Regression for rbaumier/comply#1688 — `label.fill(text)` in a
        // Playwright spec is a Locator interaction, not an array mutation.
        let src = r#"
            const label = page.getByLabel('Label');
            await label.fill(`"Updated ${id}"`);
        "#;
        assert!(run_at(src, "e2e-tests/save-from-controls.spec.ts").is_empty());
    }

    #[test]
    fn still_flags_array_fill_in_source_file() {
        // Negative space for rbaumier/comply#1688 — a genuine
        // `arr.fill(0)` array mutation with a value, in a non-test file,
        // must still be flagged.
        assert_eq!(run_at("const arr = new Array(3); arr.fill(0);", "src/util.ts").len(), 1);
    }

    #[test]
    fn ignores_playwright_locator_fill_via_bare_variable() {
        // Regression for rbaumier/comply#3001 — `rfaField.fill("12")` where
        // `rfaField` is a Playwright Locator bound to `page.getByLabel(...)`.
        // The receiver is not array-evident (member-chain initialiser), so the
        // single-arg `.fill(value)` must not be flagged as Array.prototype.fill,
        // even outside a recognised test directory.
        let src = r#"
            function editRfa(page) {
                const rfaField = page.getByLabel("RFA (%)");
                rfaField.fill("12");
            }
        "#;
        assert!(run_at(src, "src/page-objects/lab.ts").is_empty());
    }

    #[test]
    fn ignores_playwright_locator_fill_via_locator_variable() {
        // Regression for rbaumier/comply#3001 — `locator.fill("text")` where the
        // binding initialiser is `page.locator(...)`.
        let src = r##"
            function typeInto(page) {
                const locator = page.locator("#input");
                locator.fill("text");
            }
        "##;
        assert!(run_at(src, "src/helpers/forms.ts").is_empty());
    }

    #[test]
    fn still_flags_fill_on_local_array_variable() {
        // Negative space for rbaumier/comply#3001 — a genuine array fill on a
        // module-scope array literal binding stays flagged.
        assert_eq!(run_at("const arr = [1, 2, 3]; arr.fill(0);", "src/util.ts").len(), 1);
    }

    #[test]
    fn flags_push_outside_loop() {
        let src = r#"const xs = []; xs.push(1);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_next_router_push_bound_to_use_router() {
        // Regression for rbaumier/comply#1692 — `router.push(url)` where
        // `router` is bound to `useRouter()` is Next.js navigation, not an
        // Array.prototype.push mutation.
        let src = r#"
            import { useRouter } from 'next/navigation';
            export const Default = {
                play: async () => {
                    const router = useRouter();
                    router.push('/push-html', { forceOptimisticNavigation: true });
                },
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_history_push_bound_to_factory() {
        // Regression for rbaumier/comply#1692 — `history.push(url)` where the
        // receiver is bound to a history factory (React Router v5) is
        // navigation, not an array mutation. The local is named `nav` so only
        // the binding shape can recognise it.
        let src = r#"
            function go() {
                const nav = createBrowserHistory();
                nav.push('/dashboard');
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_router_push_on_named_receiver() {
        // Regression for rbaumier/comply#1692 — a `router` receiver that
        // cannot be traced to a factory (a prop / parameter) is exempt by the
        // conventional identifier name.
        let src = r#"
            function navigate(router) {
                router.push('/home');
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_array_push_on_non_router_identifier() {
        // Negative space for rbaumier/comply#1692 — a genuine `arr.push(x)`
        // array mutation outside any loop, on an array-typed identifier that is
        // neither a conventional navigation name nor bound to a navigation
        // factory, must still be flagged.
        let src = r#"
            function collect(arr: number[]) {
                arr.push(1);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_push_inside_for_of_loop_accumulator() {
        // Regression for rbaumier/comply#36 — bounded local accumulator.
        let src = r#"
            function f(rows) {
                const items = [];
                for (const row of rows) {
                    items.push(row.id);
                }
                return items;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_while_loop() {
        let src = r#"
            function f() {
                const out = [];
                let i = 0;
                while (i < 10) {
                    out.push(i);
                    i++;
                }
                return out;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_chained_receiver_push() {
        // .foo().push() — receiver is a call, not a local identifier.
        let src = r#"function f() { for (const x of xs) state.items.push(x); }"#;
        assert!(!run(src).is_empty());
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
    fn ignores_push_on_locally_declared_array_literal() {
        // Regression for rbaumier/comply#1205 — the drizzle-valibot
        // accumulator: a local array built up with conditional pushes and
        // then spread. The mutation is not externally observable.
        let src = r#"
            function numberSchema(min, max, integer, regex) {
                const actions: any[] = [v.minValue(min), v.maxValue(max)];
                if (integer) {
                    actions.push(v.integer());
                }
                if (regex) {
                    actions.push(v.regex(regex));
                }
                return v.pipe(v.number(), ...actions);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_sort_on_locally_declared_array_literal() {
        // #1205 — other mutating methods on a local array are equally
        // unobservable.
        let src = r#"
            function sorted(items) {
                const out = [...items];
                out.sort();
                return out;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_on_local_new_array() {
        // #1205 — a `new Array(...)` initialiser is just as locally owned as
        // an array literal.
        let src = r#"
            function build() {
                const xs = new Array();
                xs.push(1);
                return xs;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_push_on_parameter_array() {
        // Negative space for #1205 — a parameter array may be the caller's
        // array, so the mutation is externally observable and must still fire.
        let src = r#"
            function append(arr: number[]) {
                arr.push(1);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_push_on_member_property() {
        // Negative space for #1205 — `this.items.push(x)` mutates shared
        // object state; the receiver is not a plain local identifier.
        let src = r#"
            class Store {
                items: number[] = [];
                add(x: number) {
                    this.items.push(x);
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_push_on_module_scope_array() {
        // Negative space for #1205 — a module-scope array is reachable by
        // other code in the module, so its mutation stays observable.
        let src = r#"
            const registry: number[] = [];
            export function register(x: number) {
                registry.push(x);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_reverse_on_chained_slice_call() {
        // Regression for rbaumier/comply#3831 — `children.slice(0, n).reverse()`
        // (mui AvatarGroup) mutates the fresh array returned by `.slice()`, which
        // nothing else references. The "reverse a copy" idiom is not observable.
        let src = r#"
            function reversedHead(children, maxAvatars) {
                return children
                    .slice(0, maxAvatars)
                    .reverse()
                    .map((child) => child);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_sort_on_chained_filter_call() {
        // Regression for rbaumier/comply#3831 — `items.filter(...).sort(...)`
        // sorts the fresh array returned by `.filter()`.
        let src = r#"
            function topThree(items) {
                return items.filter((x) => x > 0).sort((a, b) => a - b);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_reverse_on_plain_array_identifier() {
        // Negative space for #3831 — `arr.reverse()` on an array-typed identifier
        // (not a fresh-array call) mutates a possibly-shared array, so it fires.
        let src = r#"
            function flip(arr: number[]) {
                arr.reverse();
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_reverse_on_non_fresh_array_call() {
        // Negative space for #3831 — `obj.getList().reverse()`: the receiver is a
        // call whose method is not a fresh-array producer, so `getList()` may
        // return a shared array and the mutation stays observable.
        let src = r#"
            function flip(obj) {
                obj.getList().reverse();
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_sort_on_inline_array_literal_receiver() {
        // Regression for rbaumier/comply#7227 — `[...referencedColumns].sort(...)`
        // (typeorm) sorts the freshly-allocated array literal that the spread
        // just created; nothing else references it, so the mutation is not
        // observable. The direct form of the "sort a copy" idiom.
        let src = r#"
            function ordered(referencedColumns, orderMap) {
                return [...referencedColumns].sort(
                    (a, b) => (orderMap.get(a) ?? Infinity) - (orderMap.get(b) ?? Infinity),
                );
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_reverse_on_inline_array_literal_receiver() {
        // #7227 — `[a, b, c].reverse()` reverses an inline literal in place.
        assert!(run("const r = [a, b, c].reverse();").is_empty());
    }

    #[test]
    fn ignores_splice_on_inline_array_literal_receiver() {
        // #7227 — `[...x].splice(0, 1)` splices a fresh spread copy.
        assert!(run("const s = [...x].splice(0, 1);").is_empty());
    }

    #[test]
    fn still_flags_sort_on_member_property_receiver() {
        // Negative space for #7227 — `this.items.sort()` mutates shared object
        // state; the receiver is a member access, not an inline array literal.
        let src = r#"
            class Store {
                items: number[] = [];
                order() {
                    this.items.sort();
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_sort_on_plain_array_identifier() {
        // Negative space for #7227 — `arr.sort()` on a parameter array may reorder
        // the caller's array, so it stays flagged; only a direct array-literal
        // receiver is exempt.
        let src = r#"
            function order(arr: number[]) {
                arr.sort();
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_fill_on_inline_new_array_receiver() {
        // Regression for rbaumier/comply#7630 — `new Array<number>(hidden).fill(0)`
        // (joplin LocalEmbeddingProvider) initialises a brand-new, unreferenced
        // buffer in place; the mutation is not externally observable.
        let src = r#"
            function makeVec(hidden: number) {
                return new Array<number>(hidden).fill(0);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_fill_on_inline_new_array_without_generic() {
        // #7630 — the same idiom without a generic type argument.
        assert!(run("const vec = new Array(8).fill(0);").is_empty());
    }

    #[test]
    fn ignores_sort_on_object_keys_receiver() {
        // Regression for rbaumier/comply#7630 — `Object.keys(theme).sort()`
        // (joplin themeToCss) sorts the fresh `string[]` that `Object.keys`
        // just allocated; the canonical "sorted keys" idiom, as fresh as
        // `[...x].sort()`.
        let src = r#"
            function sortedKeys(theme) {
                return Object.keys(theme).sort();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_sort_on_object_values_receiver() {
        // #7630 — `Object.values(o)` returns a fresh array too.
        assert!(run("const v = Object.values(o).sort();").is_empty());
    }

    #[test]
    fn ignores_sort_on_object_entries_receiver() {
        // #7630 — `Object.entries(o)` returns a fresh `[key, value][]`.
        assert!(run("const e = Object.entries(o).sort();").is_empty());
    }

    #[test]
    fn still_flags_push_on_object_property_receiver() {
        // Negative space for #7630 — `obj.list.push(x)` mutates a property that
        // other code may reference; only inline fresh receivers are exempt.
        let src = r#"
            function add(obj, x) {
                obj.list.push(x);
            }
        "#;
        assert_eq!(run(src).len(), 1);
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
    fn allows_push_on_module_children_issue_5256() {
        // jiti's loader registers a child module in the Node CJS dependency
        // graph: `ctx.parentModule.children.push(mod)` mutates the Node-owned
        // array in place — the module-loader contract.
        let src = r#"
            if (Array.isArray(ctx.parentModule.children) && !ctx.parentModule.children.includes(mod)) {
                ctx.parentModule.children.push(mod);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_push_on_module_instance_children_issue_5256() {
        // `mod.parent.children.push(...)` where `mod` resolves to a `new Module()`
        // instance is the same dependency-graph mutation via a `parent` segment.
        let src = r#"
            import { Module } from "node:module";
            const mod = new Module(filename);
            mod.parent.children.push(child);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_push_on_ordinary_children_array_issue_5256() {
        // Negative space: a bare `node.children.push(...)` without a `parent`
        // segment is an ordinary array mutation — still flagged.
        let src = r#"
            node.children.push(child);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_push_on_foreign_parent_children_issue_5256() {
        // Negative space: the generic `node.parent.children.push(...)` tree-walker
        // idiom stays flagged when the base is not a Module instance — only
        // `parentModule`, or a `new Module()`-rooted `parent` chain, is exempt.
        let src = r#"
            function attach(node, child) {
                node.parent.children.push(child);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_super_push_in_override_method() {
        // Regression for rbaumier/comply#7491 — `super.push(...)` inside an
        // `override async push(...)` dispatches to the superclass's overriding
        // method, never `Array.prototype.push`; a `super` receiver is
        // structurally not an array.
        let src = r#"
            class Gateway extends Base {
                override async push(spaceId: string, docId: string, updates: Buffer[], editorId: string) {
                    return await super.push(spaceId, docId, updates, editorId);
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_on_non_array_factory_binding() {
        // Regression for rbaumier/comply#7491 — `adapter.push(...)` where
        // `adapter` is bound to a non-array factory (`this.selectAdapter(...)`)
        // calls the adapter object's own `push` method, not
        // `Array.prototype.push`. The binding carries no array evidence (no
        // array-evident initialiser, no array type annotation), so it is not
        // flagged.
        let src = r#"
            class Sync {
                async run(client: string, spaceType: string, spaceId: string, docId: string, updates: Buffer[], editorId: string) {
                    const adapter = this.selectAdapter(client, spaceType);
                    const timestamp = await adapter.push(spaceId, docId, updates, editorId);
                    return timestamp;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_push_on_array_typed_binding_without_array_initializer() {
        // Negative space for rbaumier/comply#7491 — an explicit array type
        // annotation (`number[]`) is positive array evidence even when the
        // initialiser is a non-array-evident call, so a genuine `xs.push(3)`
        // mutation must stay flagged.
        let src = r#"
            function collect(): void {
                const xs: number[] = getThem();
                xs.push(3);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_pop_on_local_map_bound_array_issue_7594() {
        // Regression for rbaumier/comply#7594 — documenso pagination: a local
        // `const` bound to `data.map(...)` holds a brand-new array referenced
        // only through member reads; `.pop()` on it (the classic
        // fetch-`perPage + 1`-then-pop trick) mutates a private copy, not
        // externally observable.
        let src = r#"
            function paginate(data, perPage) {
                const parsedData = data.map((auditLog) => parse(auditLog));
                if (parsedData.length > perPage) {
                    const nextItem = parsedData.pop();
                    return nextItem.id;
                }
                return undefined;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_sort_on_inline_split_call_issue_7594() {
        // Regression for rbaumier/comply#7594 — documenso PDF signing:
        // `signatureText.split("\n").sort(...)` sorts the fresh `string[]` that
        // `.split()` just allocated; nothing else references it, the canonical
        // "sort a fresh copy" idiom.
        let src = r#"
            function longestLine(signatureText) {
                return signatureText.split("\n").sort((a, b) => b.length - a.length)[0];
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_sort_on_local_split_bound_array_issue_7594() {
        // #7594 — a local `const` bound to `str.split(...)` holds a fresh
        // `string[]`; sorting it in place is not observable.
        let src = r#"
            function order(str) {
                const lines = str.split("\n");
                lines.sort();
                return lines;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_pop_on_module_scope_map_bound_array_issue_7594() {
        // Negative space for #7594 — a module-scope `const` bound to
        // `data.map(...)` is reachable by other code in the module, so mutating
        // it stays observable. The scope guard must reject root-scope bindings
        // even when the initialiser is a fresh-array producer.
        let src = r#"
            const shared = data.map((x) => x);
            export function drain() {
                return shared.pop();
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_pop_on_local_const_bound_to_non_fresh_source_issue_7594() {
        // Negative space for #7594 — a non-root `const` with an array type
        // annotation but bound to a non-fresh source (an opaque getter call)
        // may alias a shared array, so its mutation stays flagged. Only a
        // fresh-array-producing initialiser is locally owned.
        let src = r#"
            function drain(): number | undefined {
                const rows: number[] = getRows();
                return rows.pop();
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_push_and_splice_on_vue_ref_value_array_issue_7656() {
        // Regression for rbaumier/comply#7656 — soybean-admin: a `ref([])` holds a
        // deeply-reactive array; `tabs.value.push(tab)` / `tabs.value.splice(...)`
        // is the idiomatic, referentially-stable Vue 3 update (the non-mutating
        // form reallocates the array and drops its reactive identity).
        let src = r#"
            import { ref } from 'vue'
            const tabs = ref([]);
            function addTab(tab) {
                tabs.value.push(tab);
                tabs.value.splice(0, 1);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_on_typed_vue_ref_value_array_issue_7656() {
        // #7656 — a typed `ref<RouteKey[]>([])` holds the same reactive array;
        // `excludeCacheRoutes.value.push(routeName)` is the reactive update.
        let src = r#"
            import { ref } from 'vue'
            const excludeCacheRoutes = ref<string[]>([]);
            function add(routeName) {
                excludeCacheRoutes.value.push(routeName);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_reduce_accumulator_issue_7656() {
        // Regression for rbaumier/comply#7656 — soybean-admin: `acc.push(cur)`
        // inside a `.reduce()` callback mutates the accumulator parameter, the
        // canonical reduce pattern; the accumulator is threaded through and
        // returned, never observed until `reduce` completes.
        let src = r#"
            function flatten(menus: string[]): string[] {
                return menus.reduce((acc: string[], cur) => {
                    acc.push(cur);
                    return acc;
                }, []);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_push_on_non_ref_value_array_issue_7656() {
        // Negative space for #7656 — `.value` on a plain const (not bound to a Vue
        // ref factory, no `vue` import) is an ordinary array mutation, still
        // flagged; only a genuine `<ref>.value` receiver is exempt.
        let src = r#"
            const notARef = getThing();
            notARef.value.push(1);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_push_on_plain_property_array_issue_7656() {
        // Negative space for #7656 — a plain property array (`obj.items`) is
        // neither a `<ref>.value` receiver nor a reduce accumulator, so its
        // mutation stays flagged.
        let src = r#"
            function add(obj, x) {
                obj.items.push(x);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_this_state_mutation_in_pinia_option_store_action_issue_7864() {
        // Regression for rbaumier/comply#7864 — vue-manage-system: in a Pinia
        // option store, `this.<state>` is a reactive proxy; mutating the array
        // it holds inside an `actions` method (`this.list.push(...)` /
        // `this.list.splice(...)`) is the idiomatic, referentially-stable
        // reactive update.
        let src = r#"
            import { defineStore } from 'pinia';
            export const useTabsStore = defineStore('tabs', {
                state: () => ({ list: [] }),
                actions: {
                    setTabsItem(data) {
                        this.list.push(data);
                    },
                    delTabsItem(index) {
                        this.list.splice(index, 1);
                    }
                }
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_this_state_mutation_when_define_store_not_from_pinia_issue_7864() {
        // Negative space for #7864 — a `defineStore` imported from a non-pinia
        // module carries no reactive-proxy provenance, so `this.list.push(x)`
        // inside its `actions` stays flagged.
        let src = r#"
            import { defineStore } from './not-pinia';
            export const useX = defineStore('x', {
                actions: {
                    add() {
                        this.list.push(1);
                    }
                }
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_this_state_mutation_outside_actions_object_issue_7864() {
        // Negative space for #7864 — the exemption is scoped to the `actions`
        // object; a `this.list.push(x)` inside a `getters` method is not a
        // Pinia action mutation and stays flagged.
        let src = r#"
            import { defineStore } from 'pinia';
            export const useX = defineStore('x', {
                getters: {
                    firstItem() {
                        this.list.push(1);
                        return this.list[0];
                    }
                }
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_this_mutation_in_plain_class_method_issue_7864() {
        // Negative space for #7864 — a plain `class Store` method mutating
        // `this.items` has no `defineStore` provenance, so it stays flagged
        // (parity with `still_flags_push_on_member_property`).
        let src = r#"
            class Store {
                items: number[] = [];
                add(x: number) {
                    this.items.push(x);
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_push_on_non_reduce_callback_param_issue_7656() {
        // Negative space for #7656 — the first parameter of a non-`.reduce()`
        // callback is not an accumulator; a genuine array mutation on it (the
        // param carries an array type annotation) stays flagged.
        let src = r#"
            function each(items) {
                items.forEach((acc: string[], item) => {
                    acc.push(item);
                });
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}

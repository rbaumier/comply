use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
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
        if name == "fill" && is_non_array_fill(member, call, ctx, semantic) {
            return;
        }
        if name == "push" && is_router_navigation(member, semantic) {
            return;
        }

        // Mutation of a locally-owned array is not externally observable:
        // the receiver resolves to a `const`/`let` declared in an inner
        // (non-module) scope whose initialiser is an array literal (`[...]`,
        // `[]`) or `new Array(...)`. This is the "build up an accumulator
        // then return it" pattern (`const actions = [a, b]; actions.push(c);
        // return v.pipe(...actions)`). A parameter array, a module-scope
        // array, or a property (`this.items`, `obj.list`) is not exempt —
        // those may be observed by other code.
        if matches!(&member.object, Expression::Identifier(_))
            && is_locally_owned_array(member, semantic)
        {
            return;
        }

        // Mutation of a fresh array produced by a chained array-returning call
        // is not externally observable: `children.slice(0, n).reverse()`,
        // `items.filter(...).sort(...)`, `xs.map(...).reverse()` mutate the
        // just-allocated array that the preceding call returned — nothing else
        // holds a reference to it. This is the canonical "reverse/sort a copy"
        // idiom. A receiver whose method is not a fresh-array producer
        // (`obj.getList().reverse()`) may return a shared array, so it stays
        // flagged.
        if matches!(&member.object, Expression::CallExpression(_))
            && is_array_evident_initializer(&member.object)
        {
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

/// `true` when `expr` proves its value is an array: an array literal (`[...]`,
/// `[]`), a `new Array(...)` construction, or an array-returning call
/// (`x.map(...)`, `x.filter(...)`, `x.slice(...)`, `x.concat(...)`,
/// `Array.from(...)`, `Array.of(...)`).
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
            if matches!(
                &member.object,
                Expression::Identifier(id) if id.name.as_str() == "Array"
            ) {
                return matches!(method, "from" | "of");
            }
            matches!(
                method,
                "map" | "filter" | "slice" | "concat" | "flat" | "flatMap" | "toSorted"
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

/// `true` when the member receiver is a plain identifier that resolves to a
/// `const`/`let` binding declared in an inner (non-module) scope whose
/// initialiser is an array literal (`[...]`, `[]`) or `new Array(...)`.
///
/// Such an array is locally owned: a mutation of it is not observable outside
/// the declaring function. A receiver that is a parameter or a module-scope
/// binding (no `VariableDeclarator` initialiser, or declared at the root
/// scope) is not exempt.
fn is_locally_owned_array(
    member: &oxc_ast::ast::StaticMemberExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Expression::Identifier(receiver) = &member.object else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(ref_id) = receiver.reference_id.get() else {
        return false;
    };
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    if scoping.symbol_scope_id(sym_id) == scoping.root_scope_id() {
        return false;
    }
    let decl_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    std::iter::once(nodes.kind(decl_id))
        .chain(nodes.ancestor_kinds(decl_id))
        .find_map(|kind| match kind {
            AstKind::VariableDeclarator(decl) => Some(decl.init.as_ref()),
            _ => None,
        })
        .flatten()
        .is_some_and(is_array_initializer)
}

/// `true` when `expr` is an array literal (`[...]`, `[]`) or a `new Array(...)`
/// construction.
fn is_array_initializer(expr: &Expression) -> bool {
    match expr {
        Expression::ArrayExpression(_) => true,
        Expression::NewExpression(new_expr) => {
            matches!(&new_expr.callee, Expression::Identifier(id) if id.name.as_str() == "Array")
        }
        _ => false,
    }
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
        // array mutation outside any loop, on an identifier that is neither a
        // conventional navigation name nor bound to a navigation factory, must
        // still be flagged.
        let src = r#"
            function collect(arr) {
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
        // Negative space for #3831 — `arr.reverse()` on a plain array identifier
        // (not a fresh-array call) mutates a possibly-shared array, so it fires.
        let src = r#"
            function flip(arr) {
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
}

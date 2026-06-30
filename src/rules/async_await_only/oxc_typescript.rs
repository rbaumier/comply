use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Walk up to the nearest enclosing function and report whether it is `async`.
/// A `.then()` only swaps cleanly for `await` when an `async` host is already in
/// scope; in a non-async function (or at module top level) switching to `await`
/// would force the function signature to change, so the chain is the natural form.
fn inside_async_function<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(f) => return f.r#async,
            AstKind::ArrowFunctionExpression(a) => return a.r#async,
            _ => {}
        }
    }
    false
}

/// True when the receiver chain bottoms out at the literal identifier `z` (Zod).
/// Syntactic only — does not resolve aliased imports (`z as zod`), variable-bound schemas (`const Schema = z.string()`), or nested `.pipe(z.x)`.
fn receiver_is_zod_chain(expr: &Expression) -> bool {
    let mut cur = expr;
    loop {
        match cur {
            Expression::Identifier(id) => return id.name.as_str() == "z",
            Expression::StaticMemberExpression(m) => cur = &m.object,
            Expression::ComputedMemberExpression(m) => cur = &m.object,
            Expression::CallExpression(c) => cur = &c.callee,
            Expression::TSNonNullExpression(n) => cur = &n.expression,
            Expression::ParenthesizedExpression(p) => cur = &p.expression,
            _ => return false,
        }
    }
}

/// `Promise.all` / `Promise.allSettled` / `Promise.race` / `Promise.any` —
/// a static member access whose object is the global `Promise` identifier.
fn is_promise_combinator_callee(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "Promise"
        && matches!(
            member.property.name.as_str(),
            "all" | "allSettled" | "race" | "any"
        )
}

/// True when `node` (a `.catch()` call) is — through transparent wrappers
/// (parens, `as`, `satisfies`, `!`) — an element of the `ArrayExpression`
/// argument of a `Promise.{all,allSettled,race,any}(...)` call. There the
/// `.catch(() => fallback)` recovers one slot's rejection so the parallel
/// combinator can still aggregate the rest; no `await` form expresses that
/// per-element fallback without serializing the calls, so the chain is the
/// idiomatic shape.
fn in_promise_combinator_array<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();

    // The matched call must sit directly inside an array literal (its nearest
    // non-transparent ancestor is an `ArrayExpression`).
    let mut current = node.id();
    let array_id = loop {
        let parent = nodes.parent_node(current);
        match parent.kind() {
            AstKind::ArrayExpression(_) => break parent.id(),
            AstKind::ParenthesizedExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSNonNullExpression(_) => current = parent.id(),
            _ => return false,
        }
    };

    // That array must be the argument of a Promise combinator call. We re-check
    // `call.callee` below, so even when the array is itself the callee (`[...]()`)
    // it cannot match the `Promise.<combinator>` member shape — argument position
    // is implied by a matching callee.
    let mut current = array_id;
    loop {
        let parent = nodes.parent_node(current);
        match parent.kind() {
            AstKind::CallExpression(call) => {
                return is_promise_combinator_callee(&call.callee);
            }
            AstKind::ParenthesizedExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSNonNullExpression(_) => current = parent.id(),
            _ => return false,
        }
    }
}

/// True when the nearest function/arrow boundary enclosing `node` (a `.catch()`
/// call) is a non-`async` executor of `new Promise((resolve, reject) => {...})`
/// — that boundary is the first argument of a `NewExpression` whose callee is
/// the identifier `Promise`. The executor is intentionally synchronous (an
/// `async` executor is a Promise-constructor anti-pattern), so
/// `chain.then(resolve).catch(reject)` is the only way to forward the inner
/// promise's settlement to the outer `resolve`/`reject`; no `await` rewrite
/// preserves the semantics without restructuring the function. A `.catch()`
/// whose nearest host is an `async` function — where `try`/`await` is the
/// correct rewrite — is not exempt; this `async`-host exclusion is the only
/// part shared with `.then()`. Unlike `.then()` (exempt in any non-async host),
/// this exemption additionally requires the non-async host to be a `Promise`
/// executor.
fn in_promise_executor<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let (is_async, fn_span) = match ancestor.kind() {
            AstKind::Function(f) => (f.r#async, f.span),
            AstKind::ArrowFunctionExpression(a) => (a.r#async, a.span),
            _ => continue,
        };
        if is_async {
            return false;
        }
        let AstKind::NewExpression(new_expr) = nodes.parent_node(ancestor.id()).kind() else {
            return false;
        };
        return matches!(&new_expr.callee, Expression::Identifier(id) if id.name.as_str() == "Promise")
            && new_expr.arguments.first().is_some_and(|arg| arg.span() == fn_span);
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".then(", ".catch("])
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

        let method = member.property.name.as_str();
        if method != "then" && method != "catch" {
            return;
        }

        // Zod `.catch`/`.then` are schema combinators — flagging them is a false positive.
        if receiver_is_zod_chain(&member.object) {
            return;
        }

        // A terminal `.catch(handler)` directly under `await` or `void` is the
        // canonical error-fallback / fire-and-forget idiom, not a promise chain
        // that should become `try/catch`:
        //   - `await promise.catch(() => fallback)` — default-on-failure
        //   - `void promise.catch(() => {})` — fire-and-forget with handled rejection
        // The alternative (`Promise.allSettled([p])`) is strictly worse.
        if method == "catch" {
            let parent = semantic.nodes().parent_node(node.id()).kind();
            let exempt = matches!(parent, AstKind::AwaitExpression(_))
                || matches!(parent, AstKind::UnaryExpression(u)
                    if u.operator == oxc_ast::ast::UnaryOperator::Void);
            if exempt {
                return;
            }

            // A fire-and-forget `.catch()` statement at module top level (e.g.
            // Angular's canonical `bootstrapApplication(App, appConfig)
            // .catch((err) => console.error(err))` in main.ts) has no enclosing
            // async function to host an `await`, and top-level await is not
            // available in every bundling context — the bare `.catch()`
            // statement is the idiomatic form there. Only the discarded-result
            // statement shape is exempt: a `.catch()` whose value is used
            // (assigned, passed as an argument) is still flagged.
            if matches!(parent, AstKind::ExpressionStatement(_))
                && !semantic.nodes().ancestor_kinds(node.id()).any(|kind| {
                    matches!(
                        kind,
                        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
                    )
                })
            {
                return;
            }

            // A `.catch(() => fallback)` that is an element of the array passed
            // to `Promise.all` / `Promise.allSettled` / `Promise.race` /
            // `Promise.any` makes one parallel slot's rejection non-fatal so the
            // combinator can still aggregate the rest. Rewriting it with
            // `try/catch` would force serial `await`s, destroying the
            // parallelism — the chain is idiomatic here.
            if in_promise_combinator_array(node, semantic) {
                return;
            }

            // A `.catch(reject)` inside the executor of `new Promise((resolve,
            // reject) => {...})` forwards the inner promise's rejection to the
            // outer promise's `reject`. The executor is intentionally
            // synchronous, so `chain.then(resolve).catch(reject)` is the only
            // correct forwarding — there is no `await` rewrite that preserves
            // the semantics. Like `.then()` below, an async host is never exempt
            // (try/await fits there); unlike `.then()`, a non-async host is
            // exempt only when it is a `Promise` executor.
            if in_promise_executor(node, semantic) {
                return;
            }
        }

        // React.lazy() requires a sync callback returning a Promise — the .then()
        // reshapes the module object and cannot be replaced with await.
        if crate::oxc_helpers::is_react_lazy_factory_then(node, semantic) {
            return;
        }

        // A `.then()` only swaps cleanly for `await` when the nearest enclosing
        // function is already `async`. In a non-async arrow/function (e.g. the
        // Angular Router `loadComponent: () => import("...").then(m => m.X)`
        // module-reshaping factory) or at module top level, `.then()` is the
        // natural form — promoting `await` there would force a signature change.
        // `.catch()` keeps its own dedicated exemptions above and is unaffected.
        if method == "then" && !inside_async_function(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{method}()` chain — prefer `async`/`await` for readability."
            ),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_then_chain_in_async_fn() {
        // Inside an async function `await` is a drop-in replacement for `.then`.
        assert_eq!(run("async function f() { foo().then((x) => x + 1); }").len(), 1);
    }

    #[test]
    fn allows_then_in_non_async_arrow() {
        // Regression for #2292: a non-async arrow that reshapes a dynamic import
        // via `.then()` (Angular Router `loadComponent`/`loadChildren` factory)
        // has no async host — `.then()` is the natural form, not flagged.
        let src = "const routes = [{ loadComponent: () => import('./x').then((c) => c.X) }];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_unawaited_catch() {
        assert_eq!(run("const p = foo().catch(() => null);").len(), 1);
    }

    #[test]
    fn allows_awaited_catch_fallback() {
        // Regression for issue #561: `await x.catch(() => null)` is the canonical
        // error-fallback on a single awaited operation, already async/await style.
        let src = "async function f() { const b = await response.clone().json().catch(() => null); return b; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_voided_catch_fire_and_forget() {
        // Regression for issue #562: `void p.catch(() => {})` is the canonical
        // fire-and-forget idiom with a handled rejection.
        let src = "void navigator.clipboard.writeText(s).catch(() => {});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_top_level_fire_and_forget_catch() {
        // Regression for issue #978: Angular's canonical standalone bootstrap in
        // main.ts runs at module top level where no async host exists for `await`.
        let src = "bootstrapApplication(App, appConfig).catch((err) => console.error(err));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_fire_and_forget_catch_inside_function() {
        // Inside a function body an `await` host is one `async` keyword away —
        // the top-level exemption does not apply.
        let src = "function f() { foo().catch(() => {}); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_awaited_then_chain() {
        // `.then` directly awaited is still a transform-chain the rule targets.
        let src = "async function f() { return await foo().then((x) => x + 1); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_catch_inside_promise_all_array() {
        // Regression for issue #6451: a `.catch(() => null)` element of the
        // array passed to `Promise.all([...])` makes one parallel slot's
        // rejection non-fatal — no `await` form expresses that per-element
        // recovery without serializing the calls.
        let src = "async function f(event: E) {\n\
                   \x20\x20const [session, user] = await Promise.all([\n\
                   \x20\x20\x20\x20serverSupabaseSession(event).catch(() => null),\n\
                   \x20\x20\x20\x20serverSupabaseUser(event).catch(() => null),\n\
                   \x20\x20]);\n\
                   \x20\x20return [session, user];\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_catch_inside_other_promise_combinators() {
        // `allSettled` / `race` / `any` array elements get the same exemption.
        for combinator in ["allSettled", "race", "any"] {
            let src = format!(
                "async function f() {{ return await Promise.{combinator}([p().catch(() => null), q()]); }}"
            );
            assert!(run(&src).is_empty(), "combinator {combinator} should be exempt");
        }
    }

    #[test]
    fn still_flags_catch_in_plain_array() {
        // An array that is not a Promise-combinator argument keeps flagging.
        let src = "function f() { const xs = [p().catch(() => null)]; return xs; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_catch_in_non_promise_all_array() {
        // The receiver must be the global `Promise`: a same-named method on
        // another object (e.g. a Bluebird-like `all`) is not exempt.
        let src = "function f() { return Bluebird.all([p().catch(() => null)]); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_catch_in_non_combinator_promise_call() {
        // A non-combinator `Promise` method (e.g. `Promise.resolve`) does not
        // aggregate an array of promises, so the element stays flagged.
        let src = "function f() { return Promise.resolve([p().catch(() => null)]); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_catch_in_promise_executor() {
        // Regression for issue #6819 (sindresorhus/ky timeout.ts): the executor
        // of `new Promise((resolve, reject) => {...})` is intentionally
        // synchronous, so `.then(resolve).catch(reject)` is the only way to
        // forward settlement to the outer promise — no `await` rewrite preserves
        // the semantics. The `.catch(reject)` sits mid-chain, so none of the
        // await/void/top-level/combinator exemptions cover it.
        let src = "async function timeout(request, init, options) {\n\
                   \x20\x20return new Promise((resolve, reject) => {\n\
                   \x20\x20\x20\x20void options.fetch(request, init)\n\
                   \x20\x20\x20\x20\x20\x20.then(resolve)\n\
                   \x20\x20\x20\x20\x20\x20.catch(reject)\n\
                   \x20\x20\x20\x20\x20\x20.then(() => { clearTimeout(id); });\n\
                   \x20\x20});\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_catch_in_async_promise_executor() {
        // An `async` executor is a Promise-constructor anti-pattern, and inside
        // it `try`/`await` is the correct rewrite — the `.catch()` stays flagged.
        let src =
            "function f() { return new Promise(async (resolve, reject) => { foo().catch(reject); }); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_catch_in_non_promise_constructor_executor() {
        // The constructor must be `Promise`: a callback to another constructor
        // is not a Promise executor, so the `.catch()` stays flagged.
        let src = "function f() { return new Foo((resolve, reject) => { bar().catch(reject); }); }";
        assert_eq!(run(src).len(), 1);
    }
}

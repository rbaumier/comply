//! Shared heuristics for the `assertions-in-tests` rule.

use crate::rules::backend::AstKind;
use oxc_ast::ast::{BindingPattern, CallExpression, Expression};
use oxc_semantic::NodeId;
use oxc_span::{GetSpan, Span};
use rustc_hash::FxHashSet;

/// How many call-graph edges into same-file helpers the assertion search
/// follows. The test body is depth 0; a helper it calls is depth 1; a helper
/// that helper calls is depth 2. Three hops covers realistic test-helper nesting
/// while bounding the work even on pathological mutual recursion (the visited
/// set already prevents revisiting a helper, so this only caps fresh chains).
const MAX_HELPER_DEPTH: u32 = 3;

/// React render functions that throw when the component/hook crashes. A test
/// whose body is only `render(<App/>)`, `renderToString(<C/>)`, or
/// `renderHook(() => useForm())` is a "does not throw" smoke test: reaching the
/// end means rendering succeeded, so the render call *is* the assertion. React
/// Testing Library and `react-dom/server` document these as valid no-throw
/// tests, so a body relying solely on them is not assertion-less.
pub(crate) const RENDER_ASSERTION_CALLS: &[&str] =
    &["render", "renderToString", "renderToStaticMarkup", "renderHook"];

/// The `node:assert` named assertion functions. Destructured and called
/// directly (`strictEqual(a, b)`, `deepStrictEqual(actual, expected)`), they
/// throw `AssertionError` on a failed check, so a test whose only assertion is
/// such a call does assert. Wrappers like `@effect/vitest/utils` re-export this
/// same API, so keying off the import source is unreliable — the function names
/// are the stable signal. This is the canonical `node:assert` surface, not an
/// arbitrary allow-list.
pub(crate) const NODE_ASSERT_FUNCTIONS: &[&str] = &[
    "strictEqual",
    "notStrictEqual",
    "deepStrictEqual",
    "notDeepStrictEqual",
    "deepEqual",
    "notDeepEqual",
    "equal",
    "notEqual",
    "ok",
    "match",
    "doesNotMatch",
    "throws",
    "doesNotThrow",
    "rejects",
    "doesNotReject",
    "ifError",
    "fail",
];

/// True when `name` is one of the [`NODE_ASSERT_FUNCTIONS`] — a `node:assert`
/// named assertion function recognised regardless of which module re-exports it.
pub(crate) fn is_node_assert_function(name: &str) -> bool {
    NODE_ASSERT_FUNCTIONS.contains(&name)
}

/// True when the file imports anything from `@playwright/test`. Playwright E2E
/// tests pull their `test`/`expect` from that module and rely on Playwright's
/// own assertion and auto-waiting model (`await expect(locator).toBeVisible()`,
/// `page.waitForSelector(...)`); action-only tests need no `expect(...)` at all.
/// Such files therefore do not follow the vitest/jest "every test asserts"
/// contract, so the test-assertion rules must not fire on them. Detected from
/// the import source specifier so it is independent of filename or identifiers.
pub(crate) fn imports_playwright_test(semantic: &oxc_semantic::Semantic<'_>) -> bool {
    semantic.nodes().iter().any(|n| {
        let AstKind::ImportDeclaration(import) = n.kind() else {
            return false;
        };
        import.source.value.as_str() == "@playwright/test"
    })
}

/// True when `call` is a Cypress assertion: a `.should(...)` or `.and(...)`
/// member call whose receiver chain is rooted at the global `cy` identifier.
/// Cypress expresses assertions by chaining `should`/`and` onto a command
/// rooted at `cy` (`cy.get(x).should(...)`, `cy.get(x).find(y).should(...)`,
/// `cy.get(x).should(...).and(...)`), so reaching such a call means the test
/// does assert. The chain is walked down its `object` links — through both
/// member accesses (`cy.get`) and the calls between them (`cy.get(x)`) — to the
/// left-most object, which must be the identifier `cy`. A bare `.should(...)` on
/// any other receiver is deliberately not matched.
pub(crate) fn is_cypress_assertion_call(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "should" | "and") {
        return false;
    }
    member_chain_root_is_cy(&member.object)
}

/// Follow the `object` links of a member/call chain down to its left-most
/// object expression and return true when that root is the identifier `cy`.
fn member_chain_root_is_cy(expr: &Expression) -> bool {
    let mut current = expr.get_inner_expression();
    loop {
        match current {
            Expression::Identifier(id) => return id.name.as_str() == "cy",
            Expression::StaticMemberExpression(member) => {
                current = member.object.get_inner_expression();
            }
            Expression::ComputedMemberExpression(member) => {
                current = member.object.get_inner_expression();
            }
            Expression::CallExpression(call) => {
                current = call.callee.get_inner_expression();
            }
            _ => return false,
        }
    }
}

/// True when the test callback (2nd argument of an `it`/`test` call) invokes an
/// identifier bound to a formal parameter of an *enclosing* function. Such a
/// test is a factory whose real body — and its assertions — is supplied by the
/// wrapper's callers:
///
/// ```ts
/// function txIt(name: string, fn: () => Promise<void>): void {
///   it(name, async () => { await fn(); }); // assertions live inside `fn`
/// }
/// ```
///
/// The inline `it` body having no `expect(...)` is therefore not a missing
/// assertion. The test callback's own parameters do not count — only those of
/// functions that lexically enclose the `it`/`test` call.
pub(crate) fn delegates_to_outer_param(
    it_call: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();

    // Functions enclosing the it() call — i.e. wrappers OUTSIDE the test
    // callback (the callback is a child of the call, never an ancestor).
    let outer_fns: FxHashSet<NodeId> = nodes
        .ancestors(it_call.id())
        .filter(|a| {
            matches!(
                a.kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            )
        })
        .map(|a| a.id())
        .collect();
    if outer_fns.is_empty() {
        return false;
    }

    let AstKind::CallExpression(call) = it_call.kind() else {
        return false;
    };
    let (lo, hi) = (call.span.start, call.span.end);

    for node in nodes.iter() {
        let AstKind::CallExpression(inner) = node.kind() else {
            continue;
        };
        if inner.span.start < lo || inner.span.end > hi {
            continue;
        }
        let Expression::Identifier(callee) = &inner.callee else {
            continue;
        };
        let Some(ref_id) = callee.reference_id.get() else {
            continue;
        };
        let Some(sym_id) = semantic.scoping().get_reference(ref_id).symbol_id() else {
            continue;
        };
        let decl = semantic.scoping().symbol_declaration(sym_id);
        if binding_is_outer_param(decl, &outer_fns, semantic) {
            return true;
        }
    }
    false
}

/// True when the test body (the `it`/`test` call span) invokes an identifier
/// bound to a formal parameter of a callback the test itself passes to *another*
/// call. The parameter's value is supplied by that outer callee — typically a
/// cross-file custom tester helper — so its assertions live in another module
/// and are invisible to this per-file pass:
///
/// ```ts
/// it("inserts", async () => {
///   await knex("accounts")
///     .insert({ ... }, "id")
///     .testSql(function (tester) {        // `tester` is supplied by `.testSql`
///       tester("mysql", "insert into ...", [...], 1); // the real expect() is in
///       tester("pg", "insert into ...", [...], [{ id: 1 }]); // testSqlTester()
///     });
/// });
/// ```
///
/// This is the inverse of [`delegates_to_outer_param`]: there the test delegates
/// to a parameter of an *enclosing* wrapper; here it delegates to a parameter of
/// a callback it passes *outward*. The callback must be an argument of a separate
/// call (not the `it`/`test` call itself), so a plain non-asserting body call
/// such as `it("x", () => { doThing(); })` is not matched.
pub(crate) fn delegates_to_callback_param(
    it_call: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::CallExpression(call) = it_call.kind() else {
        return false;
    };
    let (lo, hi) = (call.span.start, call.span.end);

    for node in nodes.iter() {
        let AstKind::CallExpression(inner) = node.kind() else {
            continue;
        };
        if inner.span.start < lo || inner.span.end > hi {
            continue;
        }
        let Expression::Identifier(callee) = &inner.callee else {
            continue;
        };
        let Some(ref_id) = callee.reference_id.get() else {
            continue;
        };
        let Some(sym_id) = semantic.scoping().get_reference(ref_id).symbol_id() else {
            continue;
        };
        let decl = semantic.scoping().symbol_declaration(sym_id);
        if binding_is_outward_callback_param(decl, it_call.id(), semantic) {
            return true;
        }
    }
    false
}

/// True when `decl` is a formal parameter of a function/arrow that is passed as
/// an argument to a call expression *other than* the `it`/`test` call — i.e. a
/// callback the test hands outward whose parameter is supplied by that outer
/// callee.
fn binding_is_outward_callback_param(
    decl: NodeId,
    it_call_id: NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();

    // Find the function/arrow that owns `decl`, requiring `decl` to be a formal
    // parameter of it (a local `const`/`let` binding is not a callback param).
    let mut saw_param = matches!(
        nodes.kind(decl),
        AstKind::FormalParameter(_) | AstKind::FormalParameters(_)
    );
    let mut owner_fn = None;
    for anc in nodes.ancestors(decl) {
        match anc.kind() {
            AstKind::FormalParameter(_) | AstKind::FormalParameters(_) => saw_param = true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                owner_fn = Some(anc.id());
                break;
            }
            _ => {}
        }
    }
    let Some(owner_fn) = owner_fn else {
        return false;
    };
    if !saw_param {
        return false;
    }

    // The owning callback must be an argument of a call that is not the it/test
    // call itself — that call's callee supplies the parameter's value.
    let parent_id = nodes.parent_id(owner_fn);
    if parent_id == it_call_id {
        return false;
    }
    matches!(nodes.kind(parent_id), AstKind::CallExpression(_))
}

/// True when `call`'s callee is a bare identifier bound to the first formal
/// parameter (the `resolve` slot) of an enclosing `new Promise((resolve) => …)`
/// executor. In a promise-returning test, reaching that resolve call is the
/// implicit assertion: if it is never invoked the promise never settles and the
/// runner fails the test by timeout. A body whose only completion path calls
/// the resolve parameter is therefore not assertion-less:
///
/// ```ts
/// test("queue a task", () =>
///   new Promise(done => { requestCallback(() => { done(undefined); }); }));
/// ```
///
/// Unlike [`is_promise_reject_assertion`], no `new Error(...)` argument is
/// required — `done()` / `resolve(value)` all count.
pub(crate) fn is_promise_resolve_call(
    call: &CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    call_callee_is_promise_executor_param(call, semantic, 0)
}

/// True when `call` is `reject(new Error(...))` (or any `*Error` constructor)
/// where `reject` is bound to the second formal parameter of an enclosing
/// `new Promise((resolve, reject) => …)` executor. Reaching this rejection
/// fails the test with that error, so it *is* the assertion.
pub(crate) fn is_promise_reject_assertion(
    call: &CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // First argument must be `new Error(...)` (or any `*Error` constructor).
    let Some(oxc_ast::ast::Argument::NewExpression(new_expr)) = call.arguments.first() else {
        return false;
    };
    let Expression::Identifier(ctor) = &new_expr.callee else {
        return false;
    };
    if !ctor.name.ends_with("Error") {
        return false;
    }
    call_callee_is_promise_executor_param(call, semantic, 1)
}

/// True when `call`'s callee is a bare identifier bound to the formal parameter
/// at `param_index` of an enclosing `new Promise(...)` executor.
fn call_callee_is_promise_executor_param(
    call: &CallExpression,
    semantic: &oxc_semantic::Semantic,
    param_index: usize,
) -> bool {
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    let Some(ref_id) = callee.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl = scoping.symbol_declaration(sym_id);
    declaration_is_promise_executor_param(decl, semantic, param_index)
}

/// True when `decl` is the formal parameter at `param_index` of a function
/// passed as the executor to `new Promise(...)`.
fn declaration_is_promise_executor_param(
    decl: NodeId,
    semantic: &oxc_semantic::Semantic,
    param_index: usize,
) -> bool {
    let nodes = semantic.nodes();

    // Find the enclosing function and the binding's span.
    let decl_span = nodes.kind(decl).span();
    let executor_id = std::iter::once(nodes.get_node(decl))
        .chain(nodes.ancestors(decl))
        .find(|anc| {
            matches!(
                anc.kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            )
        })
        .map(|anc| anc.id());
    let Some(executor_id) = executor_id else {
        return false;
    };

    // The executor's parent must be `new Promise(...)`.
    let parent_id = nodes.parent_id(executor_id);
    let AstKind::NewExpression(new_expr) = nodes.kind(parent_id) else {
        return false;
    };
    let Expression::Identifier(ctor) = &new_expr.callee else {
        return false;
    };
    if ctor.name.as_str() != "Promise" {
        return false;
    }

    // The binding must be the formal parameter at `param_index`.
    let params = match nodes.kind(executor_id) {
        AstKind::Function(f) => &f.params,
        AstKind::ArrowFunctionExpression(f) => &f.params,
        _ => return false,
    };
    params.items.get(param_index).is_some_and(|param| {
        param.span.start <= decl_span.start && decl_span.end <= param.span.end
    })
}

/// True when a call within `body_span` resolves to a same-file helper function
/// (defined at module scope or any enclosing `describe`/function scope) whose
/// own body asserts. The assertion may sit directly in that helper or, in turn,
/// in a further same-file helper it calls.
///
/// This recognises the common shared-assertion pattern where several tests
/// delegate their only `expect(...)` to a locally-defined closure:
///
/// ```ts
/// describe("Cache-Control header", () => {
///   const shouldNotSetCacheControlHeader = (response) => {
///     expect(response.headers.get("cache-control")).toBeUndefined();
///   };
///   it("is not set", async () => {
///     const response = await makePlugin();
///     shouldNotSetCacheControlHeader(response); // assertion lives in the helper
///   });
/// });
/// ```
///
/// Only plain-identifier callees bound to a function/arrow/function-expression
/// declared in the same file are followed; imported symbols and parameters are
/// not chased. The search is bounded by [`MAX_HELPER_DEPTH`] and a visited set
/// of helper declarations, so self-recursive and mutually-recursive helpers
/// terminate.
pub(crate) fn body_calls_asserting_local_helper(
    body_span: Span,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut visited = FxHashSet::default();
    span_reaches_asserting_helper(body_span, semantic, &mut visited, 0)
}

/// True when a call within `body_span` invokes an assertion entry point by name:
/// a bare or `expect`/`assert`-prefixed identifier (`expect(...)`,
/// `expectSelectedTab(...)`, `assertResponse(...)`), `attest`, a React render
/// call, or an `expect.*`/`assert.*` member call. This is the naming-convention
/// signal — it fires even when the helper is imported from another file, where
/// [`body_calls_asserting_local_helper`] (which only follows same-file
/// declarations) cannot see the assertion. Mirrors eslint-plugin-playwright /
/// jest's `assertFunctionNames` defaults.
pub(crate) fn body_contains_assertion_prefixed_call(
    body_span: Span,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    semantic.nodes().iter().any(|n| {
        let AstKind::CallExpression(call) = n.kind() else {
            return false;
        };
        call.span.start >= body_span.start
            && call.span.end <= body_span.end
            && call_is_assertion(call)
    })
}

/// Walk every call expression inside `span`; if any resolves to a same-file
/// helper whose body asserts (directly or transitively), return true.
fn span_reaches_asserting_helper(
    span: Span,
    semantic: &oxc_semantic::Semantic,
    visited: &mut FxHashSet<NodeId>,
    depth: u32,
) -> bool {
    if depth >= MAX_HELPER_DEPTH {
        return false;
    }
    let nodes = semantic.nodes();
    for node in nodes.iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        if call.span.start < span.start || call.span.end > span.end {
            continue;
        }
        let Expression::Identifier(callee) = &call.callee else {
            continue;
        };
        let Some(ref_id) = callee.reference_id.get() else {
            continue;
        };
        let Some(sym_id) = semantic.scoping().get_reference(ref_id).symbol_id() else {
            continue;
        };
        let decl = semantic.scoping().symbol_declaration(sym_id);
        // Skip a helper already on the current path (recursion guard).
        if !visited.insert(decl) {
            continue;
        }
        if let Some(helper_body) = local_function_body_span(decl, semantic)
            && (span_has_direct_assertion(helper_body, semantic)
                || span_reaches_asserting_helper(helper_body, semantic, visited, depth + 1))
        {
            return true;
        }
    }
    false
}

/// Map a declaration node to the body span of the function it binds, when that
/// declaration is a same-file function definition: `function f() { … }`,
/// `const f = () => { … }`, or `const f = function () { … }`. Returns `None` for
/// any other binding (imports have no declaration node in this file's AST, so
/// they are excluded for free).
fn local_function_body_span(decl: NodeId, semantic: &oxc_semantic::Semantic) -> Option<Span> {
    let nodes = semantic.nodes();
    match nodes.kind(decl) {
        AstKind::Function(func) => func.body.as_ref().map(|b| b.span),
        AstKind::VariableDeclarator(declarator) => {
            let BindingPattern::BindingIdentifier(_) = &declarator.id else {
                return None;
            };
            match declarator.init.as_ref()? {
                Expression::ArrowFunctionExpression(arrow) => Some(arrow.body.span),
                Expression::FunctionExpression(func) => func.body.as_ref().map(|b| b.span),
                _ => None,
            }
        }
        _ => None,
    }
}

/// True when an assertion AST node lies directly within `span`: a call to an
/// `expect`/`assert`-prefixed or `attest` identifier, an `expect.*`/`assert.*`
/// member call, a React render call, a matcher member (`.toBe`/`.toEqual`/…), a
/// `throw` statement, or a `satisfies` expression. Mirrors the assertion signals
/// both test-assertion rules accept; nested helper calls are followed by the
/// caller, not here.
fn span_has_direct_assertion(span: Span, semantic: &oxc_semantic::Semantic) -> bool {
    semantic.nodes().iter().any(|n| {
        let in_span = |s: Span| s.start >= span.start && s.end <= span.end;
        match n.kind() {
            AstKind::CallExpression(call) if in_span(call.span) => {
                call_is_assertion(call) || is_cypress_assertion_call(call)
            }
            AstKind::StaticMemberExpression(member) if in_span(member.span) => matches!(
                member.property.name.as_str(),
                "toBe" | "toEqual" | "toMatch" | "toThrow"
            ),
            AstKind::ThrowStatement(throw) if in_span(throw.span) => true,
            AstKind::TSSatisfiesExpression(sat) if in_span(sat.span) => true,
            _ => false,
        }
    })
}

/// True when a call's callee names an assertion entry point: an `expect`/`assert`
/// identifier (or prefixed helper like `expectProblem`), `attest`, a React
/// render call, or an `expect.*`/`assert.*` member call.
fn call_is_assertion(call: &CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            name.starts_with("expect")
                || name.starts_with("assert")
                || name == "attest"
                || RENDER_ASSERTION_CALLS.contains(&name)
                || is_node_assert_function(name)
        }
        Expression::StaticMemberExpression(member) => {
            let object = member.object.get_inner_expression();
            matches!(
                object,
                Expression::Identifier(obj)
                    if obj.name.as_str() == "expect" || obj.name.as_str() == "assert"
            )
        }
        _ => false,
    }
}

/// True when `decl` is a formal-parameter binding of one of `outer_fns`.
fn binding_is_outer_param(
    decl: NodeId,
    outer_fns: &FxHashSet<NodeId>,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut saw_param = matches!(
        nodes.kind(decl),
        AstKind::FormalParameter(_) | AstKind::FormalParameters(_)
    );
    for anc in nodes.ancestors(decl) {
        match anc.kind() {
            AstKind::FormalParameter(_) | AstKind::FormalParameters(_) => saw_param = true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return saw_param && outer_fns.contains(&anc.id());
            }
            _ => {}
        }
    }
    false
}

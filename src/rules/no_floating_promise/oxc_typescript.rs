use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

use super::shared::ASYNC_LOOKING_METHODS;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExpressionStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // `*.test-d.{ts,tsx}` are tsd / `expect-type` type-declaration tests:
        // call statements there are type assertions checked by `tsc --noEmit`,
        // never executed, so a "floating" promise can never reject at runtime.
        if crate::rules::path_utils::has_test_d_infix(ctx.path) {
            return;
        }

        let AstKind::ExpressionStatement(stmt) = node.kind() else {
            return;
        };

        // OXC normalises an arrow's concise body (`() => expr`) into a synthetic
        // ExpressionStatement (sometimes wrapped in a FunctionBody) under an
        // ArrowFunctionExpression with `expression == true`. Such a node is the
        // function's implicit return value, not a discarded statement, so a
        // promise it produces is handed back to the caller, not floated.
        if is_concise_arrow_body(node, semantic) {
            return;
        }
        let Expression::CallExpression(call) = &stmt.expression else {
            return;
        };

        // Check if already handled by .then/.catch/.finally
        if has_promise_handler(call) {
            return;
        }

        let is_flag = is_promise_combinator(call) || is_async_looking_member_call(call, ctx);
        if !is_flag {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Promise-returning call is used as a statement \u{2014} rejections will \
                      become UnhandledPromiseRejection. Add `await`, chain `.catch`, \
                      or prefix with `void` if you really want to ignore it."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

use oxc_ast::ast::*;

/// True when `node` (an ExpressionStatement) is the synthetic body of a
/// concise-body arrow function — its grandparent (through the optional
/// `FunctionBody` wrapper) is an `ArrowFunctionExpression` with
/// `expression == true`, meaning the expression is the arrow's implicit return.
fn is_concise_arrow_body(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    let arrow_node = match parent.kind() {
        AstKind::FunctionBody(_) => semantic.nodes().parent_node(parent.id()),
        AstKind::ArrowFunctionExpression(_) => parent,
        _ => return false,
    };
    matches!(
        arrow_node.kind(),
        AstKind::ArrowFunctionExpression(arrow) if arrow.expression
    )
}

/// Does the call end with `.then(...)` / `.catch(...)` / `.finally(...)`?
fn has_promise_handler(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    matches!(
        member.property.name.as_str(),
        "then" | "catch" | "finally"
    )
}

/// Is the callee `Promise.<combinator>`?
fn is_promise_combinator(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    if obj.name.as_str() != "Promise" {
        return false;
    }
    matches!(
        member.property.name.as_str(),
        "resolve" | "reject" | "all" | "allSettled" | "race" | "any"
    )
}

/// Is the callee a member whose method name is in the async-looking list?
fn is_async_looking_member_call(call: &CallExpression, ctx: &CheckCtx) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if receiver_is_cast(member) {
        return false;
    }
    if is_redux_store_dispatch(member) {
        return false;
    }
    if is_editor_view_dispatch(member) {
        return false;
    }
    if is_diagnostics_channel_publish(member) {
        return false;
    }
    if is_fluent_builder_run(member) {
        return false;
    }
    if is_audio_node_connect(call, member) {
        return false;
    }
    if is_better_sqlite3_sync_method(member, ctx) {
        return false;
    }
    if is_threejs_sync_method(member, ctx) {
        return false;
    }
    if is_express_route_dispatch(call, member) {
        return false;
    }
    let method = member.property.name.as_str();
    ASYNC_LOOKING_METHODS.contains(&method)
}

/// better-sqlite3 is a fully synchronous library: `Statement.run()` returns a
/// `RunResult` and `Database.exec()` returns the `Database` — never a Promise.
/// These are the only two heuristic-listed method names the library exposes, so
/// in a file that imports `better-sqlite3` a statement-level `.run()` / `.exec()`
/// call is synchronous and must not be flagged.
fn is_better_sqlite3_sync_method(member: &StaticMemberExpression, ctx: &CheckCtx) -> bool {
    matches!(member.property.name.as_str(), "run" | "exec")
        && ctx.source_contains("better-sqlite3")
}

/// The Three.js / react-three-fiber ecosystem reuses several heuristic-listed
/// method names for synchronous, non-Promise operations: controls and animation
/// mixers expose `connect(domElement)` (wires DOM events, returns `void`),
/// `save()` / `load()` (snapshot and restore controls state synchronously), and
/// `CanvasRenderingContext2D.save()` pushes rendering state to a stack. None of
/// these return a Promise. In a file that imports the Three.js ecosystem
/// (`three`, `@react-three/fiber`, `@react-three/drei`), a statement-level
/// `.connect()` / `.save()` / `.load()` is one of these synchronous calls and
/// must not be flagged. (`update` is already absent from the heuristic.)
fn is_threejs_sync_method(member: &StaticMemberExpression, ctx: &CheckCtx) -> bool {
    matches!(member.property.name.as_str(), "connect" | "save" | "load")
        && file_imports_threejs(ctx.source)
}

/// True when the file imports from the Three.js ecosystem — `three`,
/// `@react-three/fiber`, or `@react-three/drei` — via an ESM `import ... from`.
/// Matches the quoted specifier (not a bare substring) to avoid spurious hits on
/// unrelated identifiers, and is memoized per file via `source_contains`.
fn file_imports_threejs(source: &str) -> bool {
    const SPECIFIERS: &[&str] = &[
        "from \"three\"",
        "from 'three'",
        "from \"three/",
        "from 'three/",
        "from \"@react-three/",
        "from '@react-three/",
    ];
    SPECIFIERS
        .iter()
        .any(|s| crate::oxc_helpers::source_contains(source, s))
}

/// Web Audio's `AudioNode.prototype.connect(destination)` returns the destination
/// `AudioNode` (for chaining) or `void` — never a Promise.
/// Matches a `.connect(...)` call by either type-free syntactic signal:
///   - the receiver is itself a `.connect(...)` call, e.g.
///     `osc.connect(gain).connect(masterGain)` — a Promise has no `.connect`
///     method, so chaining `.connect()` proves the inner call returns a node; or
///   - the sole argument is an `AudioContext`/`OfflineAudioContext` sink, i.e. a
///     member access ending in `.destination`, e.g. `gain.connect(ctx.destination)`.
fn is_audio_node_connect(call: &CallExpression, member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "connect" {
        return false;
    }
    if matches!(peel_parens(&member.object), Expression::CallExpression(inner) if callee_method_is(inner, "connect"))
    {
        return true;
    }
    let Some(arg) = call.arguments.first().and_then(Argument::as_expression) else {
        return false;
    };
    matches!(peel_parens(arg), Expression::StaticMemberExpression(m) if m.property.name.as_str() == "destination")
}

/// Does `call`'s callee read as `<receiver>.<method>(...)` for the given method?
fn callee_method_is(call: &CallExpression, method: &str) -> bool {
    matches!(&call.callee, Expression::StaticMemberExpression(m) if m.property.name.as_str() == method)
}

/// A fluent command-builder `.run()` terminal — e.g. tiptap's
/// `editor.chain().focus().toggleBold().run()` — returns `boolean` (whether the
/// queued commands applied), not a Promise, so it must not be flagged.
/// Matches `.run()` whose receiver is a method-call chain rooted in a `.chain()`
/// call.
fn is_fluent_builder_run(member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "run" {
        return false;
    }
    chain_is_rooted_in_chain_call(peel_parens(&member.object))
}

/// Walk a `a.b().c().d()` method-call chain from the outside in, returning true
/// when any link is a `.chain()` call.
fn chain_is_rooted_in_chain_call(expr: &Expression) -> bool {
    let mut current = expr;
    loop {
        let Expression::CallExpression(call) = current else {
            return false;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return false;
        };
        if member.property.name.as_str() == "chain" {
            return true;
        }
        current = peel_parens(&member.object);
    }
}

/// True when the call receiver is a type assertion, e.g. `(api as any).dispatch(...)`.
/// A cast erases any type basis the heuristic could rely on, so the purely
/// speculative async-looking-method match must not fire — `(foo as Bar).dispatch(x)`
/// could be a synchronous Redux/zustand-style `dispatch`.
fn receiver_is_cast(member: &StaticMemberExpression) -> bool {
    matches!(peel_parens(&member.object), Expression::TSAsExpression(_))
}

/// Unwrap any `ParenthesizedExpression` wrappers around `expr`.
fn peel_parens<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    while let Expression::ParenthesizedExpression(p) = current {
        current = &p.expression;
    }
    current
}

/// Redux's `Store.dispatch(action)` and NgRx's `Store#dispatch(action)` both
/// return synchronously (the dispatched `Action`, or `void` in NgRx) — there is
/// no Promise to await or catch.
/// Matches when the `.dispatch(...)` receiver reads as a store, either as a bare
/// identifier (`store`, `reduxStore`, `appStore`) or as a member access whose
/// terminal property reads as a store (`this.store`, `this.appStore` — the NgRx
/// idiom where the `Store` is an injected field).
fn is_redux_store_dispatch(member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "dispatch" {
        return false;
    }
    match &member.object {
        Expression::Identifier(id) => name_reads_as_store(id.name.as_str()),
        Expression::StaticMemberExpression(inner) => {
            name_reads_as_store(inner.property.name.as_str())
        }
        _ => false,
    }
}

/// Does an identifier or property name (case-insensitive) read as a store handle?
fn name_reads_as_store(name: &str) -> bool {
    name.to_lowercase().contains("store")
}

/// ProseMirror's `EditorView.dispatch(tr)` synchronously commits a transaction and
/// returns `void` — there is nothing to await or catch.
/// Matches `.dispatch(...)` whose receiver is a bare identifier named `view` or
/// `editorView` (`view.dispatch(tr)`), or a member access ending in `.view`
/// (`editor.view.dispatch(tr)`, `this.editor.view.dispatch(tr)`).
fn is_editor_view_dispatch(member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "dispatch" {
        return false;
    }
    match &member.object {
        Expression::Identifier(id) => {
            let name = id.name.as_str().to_lowercase();
            name == "view" || name == "editorview"
        }
        Expression::StaticMemberExpression(inner) => {
            inner.property.name.as_str().to_lowercase() == "view"
        }
        _ => false,
    }
}

/// Express's `Route.prototype.dispatch(req, res, done)` (and the equivalent
/// `Router`/`Layer` dispatch) is continuation-passing: it threads the request
/// through the middleware stack and delivers its outcome via the trailing `done`
/// callback — it returns `void`, never a Promise.
/// Matches a `.dispatch(...)` call with the Express dispatch shape: exactly three
/// arguments, whose first argument reads as a request (`req`/`request`) and whose
/// last argument is a callback (a function/arrow expression, or a bare identifier
/// reading as a continuation), e.g. `route.dispatch(req, {}, done)` /
/// `route.dispatch(req, {}, (err) => {})`. The request-shaped head plus a trailing
/// callback rules out a Promise return, so the call must not be flagged.
fn is_express_route_dispatch(call: &CallExpression, member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "dispatch" || call.arguments.len() != 3 {
        return false;
    }
    let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
        return false;
    };
    if !matches!(peel_parens(first), Expression::Identifier(id) if name_reads_as_request(id.name.as_str()))
    {
        return false;
    }
    let Some(last) = call.arguments.last().and_then(Argument::as_expression) else {
        return false;
    };
    matches!(
        peel_parens(last),
        Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
    ) || matches!(peel_parens(last), Expression::Identifier(id) if name_reads_as_callback(id.name.as_str()))
}

/// Does an identifier name (case-insensitive) read as an HTTP request handle?
fn name_reads_as_request(name: &str) -> bool {
    let name = name.to_lowercase();
    name == "req" || name == "request"
}

/// Does an identifier name (case-insensitive) read as a continuation callback?
fn name_reads_as_callback(name: &str) -> bool {
    let name = name.to_lowercase();
    matches!(name.as_str(), "done" | "cb" | "callback" | "next")
}

/// `node:diagnostics_channel` `Channel.prototype.publish(message)` returns `void`
/// — it fires subscribers synchronously, so there is nothing to await.
/// Matches when the `.publish(...)` receiver is, or hangs off, a bare identifier
/// whose name (case-insensitive) reads as a diagnostics channel, e.g.
/// `channel.publish(ctx)` or `channel.error.publish(ctx)`.
fn is_diagnostics_channel_publish(member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "publish" {
        return false;
    }
    let root = match &member.object {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(inner) => match &inner.object {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return false,
        },
        _ => return false,
    };
    root.to_lowercase().contains("channel")
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
    use crate::diagnostic::Diagnostic;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_async_looking_method() {
        let d = run_on("db.save(user);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_then_chain() {
        assert!(run_on("api.fetch(url).then(handleResult);").is_empty());
    }

    // Regression tests for issue #183: `.delete(...)` on Map/Set/WeakMap/WeakSet
    // returns `boolean`, not a Promise, and must not be flagged.

    #[test]
    fn allows_map_delete_in_for_of() {
        let src = "\
const cache = new Map<string, number>();
cache.set('a', 1);
for (const [key] of cache) {
  cache.delete(key);
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_set_delete() {
        assert!(run_on("set.delete(value);").is_empty());
    }

    #[test]
    fn allows_weakmap_delete() {
        assert!(run_on("weakMap.delete(obj);").is_empty());
    }

    #[test]
    fn allows_weakset_delete() {
        assert!(run_on("weakSet.delete(obj);").is_empty());
    }

    #[test]
    fn still_flags_genuine_promise_returning_member_call() {
        let d = run_on("repo.save(entity);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #208: URLSearchParams mutator methods
    // (`delete`, `set`, `append`, `sort`) return `void` per WHATWG URL spec —
    // none should be flagged. `delete` was dropped from the heuristic in #183;
    // the others were never on the list. These tests lock that contract in.

    #[test]
    fn allows_urlsearchparams_delete() {
        let src = "\
const params = new URLSearchParams(\"?a=1\");
params.delete(\"a\");
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_urlsearchparams_set() {
        let src = "\
const params = new URLSearchParams(\"?a=1\");
params.set(\"a\", \"b\");
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_urlsearchparams_append() {
        let src = "\
const params = new URLSearchParams(\"?a=1\");
params.append(\"x\", \"y\");
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_urlsearchparams_sort() {
        let src = "\
const params = new URLSearchParams(\"?b=1&a=2\");
params.sort();
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_url_searchparams_chain_delete() {
        // Real-world pattern from the issue's repro file: parsed.searchParams.delete(key).
        let src = "\
const parsed = new URL(\"https://example.com/?a=1\");
parsed.searchParams.delete(\"a\");
";
        assert!(run_on(src).is_empty());
    }

    // Regression tests for issue #1190: `close`, `write`, `emit`, and `send` are
    // dominated by synchronous, callback-based Node.js APIs that return
    // non-Promise values — flagging them on name alone produces more false
    // positives than true positives, so they are not part of the heuristic.

    #[test]
    fn allows_server_close() {
        // The issue's exact example: `http.Server.close([cb])` returns the
        // `Server`, not a Promise.
        let src = "\
afterAll(() => {
  externalServer.close();
});
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_stream_write() {
        // `stream.write(chunk)` returns `boolean` (the backpressure signal).
        assert!(run_on("stream.write(chunk);").is_empty());
    }

    #[test]
    fn allows_emitter_emit() {
        // `EventEmitter.emit(event)` returns `boolean` (whether a listener fired).
        assert!(run_on("emitter.emit(event);").is_empty());
    }

    #[test]
    fn allows_websocket_send() {
        // `WebSocket.send(data)` returns `void`.
        assert!(run_on("ws.send(data);").is_empty());
    }

    #[test]
    fn still_flags_genuine_floating_async_method() {
        // Negative-space guard: an async-dominant method name still fires exactly
        // one diagnostic.
        let d = run_on("db.save(user);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #2051: `node:diagnostics_channel`
    // `Channel.publish(message)` returns void — it fires subscribers synchronously,
    // so `channel.X.publish(...)` must not be flagged.

    #[test]
    fn allows_diagnostics_channel_publish() {
        let src = "\
channel.error.publish(context);
channel.end.publish(context);
channel.asyncEnd.publish(context);
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_direct_channel_publish() {
        assert!(run_on("channel.publish(context);").is_empty());
    }

    #[test]
    fn still_flags_non_channel_publish() {
        let d = run_on("broker.publish(topic, message);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #1978: a method call whose receiver is a type
    // assertion (`(api as any).dispatch(...)`) gives the heuristic no type basis
    // to infer a Promise return — Redux/zustand-style `dispatch(action)` on a cast
    // receiver is synchronous — so it must not be flagged.

    #[test]
    fn allows_dispatch_on_cast_receiver() {
        assert!(run_on(";(api as any).dispatch({ type: 'INCREMENT' })").is_empty());
    }

    #[test]
    fn allows_method_on_named_cast_receiver() {
        assert!(run_on("(foo as Bar).dispatch(x);").is_empty());
    }

    #[test]
    fn still_flags_method_on_non_cast_receiver() {
        let d = run_on("emitter.dispatch(action);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #1859: Redux's `store.dispatch(action)` returns
    // the dispatched Action synchronously for a plain action object — there is no
    // Promise to await — so a `.dispatch(...)` on a store-named receiver must not
    // be flagged.

    #[test]
    fn allows_redux_store_dispatch() {
        let src = "\
const store = createRechartsStore();
store.dispatch(
  setMouseOverAxisIndex({
    activeCoordinate: { x: 3, y: 4 },
    activeDataKey: 'uv',
    activeIndex: '1',
  }),
);
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_named_store_dispatch() {
        assert!(run_on("reduxStore.dispatch(addTodo());").is_empty());
    }

    #[test]
    fn still_flags_non_store_dispatch() {
        // Control: `.dispatch(...)` on a non-store receiver (e.g. a message broker)
        // stays flagged.
        let d = run_on("emitter.dispatch(event);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #1598: NgRx's `Store#dispatch(action)` returns
    // `void` — the dominant call in Angular/NgRx apps is `this.store.dispatch(...)`,
    // where the `Store` is an injected field, so a `.dispatch(...)` on a member
    // access whose terminal property reads as a store must not be flagged.

    #[test]
    fn allows_ngrx_this_store_dispatch() {
        let src = "\
closeSidenav() {
  this.store.dispatch(LayoutActions.closeSidenav());
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_ngrx_named_injected_store_dispatch() {
        assert!(run_on("this.appStore.dispatch(AuthActions.logoutConfirmation());").is_empty());
    }

    #[test]
    fn still_flags_genuine_floating_promise_dispatch() {
        // Negative-space guard: a real Promise-returning `.dispatch(...)` on a
        // non-store member access (e.g. a job queue) stays flagged.
        let d = run_on("this.queue.dispatch(job);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #1818: ProseMirror's `EditorView.dispatch(tr)`
    // synchronously commits a transaction and returns void — there is nothing to
    // await — so a `.dispatch(...)` on a `view`/`editorView` receiver, or one
    // hanging off a `.view` member access, must not be flagged.

    #[test]
    fn allows_editor_view_dispatch() {
        let src = "\
view.dispatch(tr);
this.editor.view.dispatch(tr);
view.dispatch(transaction);
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_editor_dot_view_dispatch() {
        assert!(run_on("editor.view.dispatch(tr);").is_empty());
    }

    #[test]
    fn allows_named_editor_view_dispatch() {
        assert!(run_on("editorView.dispatch(tr);").is_empty());
    }

    // Regression tests for issue #1817: tiptap's fluent command builder
    // `editor.chain()...run()` ends in a `.run()` that returns `boolean` (whether
    // the queued commands applied), not a Promise — it must not be flagged.

    #[test]
    fn allows_tiptap_chain_run() {
        assert!(
            run_on("editor.chain().setContent('<code>test</code>').setTextSelection({ from: 2, to: 3 }).run()").is_empty()
        );
    }

    #[test]
    fn allows_tiptap_chain_run_short() {
        assert!(run_on("editor.chain().focus().toggleBold().run()").is_empty());
    }

    #[test]
    fn allows_tiptap_chain_run_with_command() {
        let src = "\
editor
  .chain()
  .command(({ tr }) => { return true; })
  .setTextSelection({ from, to })
  .focus(undefined, { scrollIntoView: false })
  .run();
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_run_without_chain() {
        // A bare `.run()` whose receiver is not a `.chain()`-rooted builder stays
        // flagged — e.g. a job/task runner.
        let d = run_on("job.run();");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #1825: `*.test-d.{ts,tsx}` are tsd /
    // `expect-type` type-declaration tests. Their call statements are type
    // assertions checked by `tsc --noEmit`, never executed, so a "floating"
    // promise can never reject at runtime — they must not be flagged.

    #[test]
    fn allows_floating_call_in_test_d_ts() {
        let src = "\
import { graphql, HttpResponse } from 'msw'

it('infers the result type', () => {
  graphql.query(
    createTypedDocumentString<{ user: { id: string; name: string } }>(''),
    () => {
      return HttpResponse.json({ data: { user: { id: '1', name: 'John Doe' } } })
    },
  )
})
";
        assert!(
            run_at(src, "test/typings/graphql-typed-document-string.test-d.ts").is_empty(),
            "floating promise in a .test-d.ts type-declaration test must not be flagged"
        );
    }

    #[test]
    fn allows_floating_call_in_test_d_tsx() {
        assert!(run_at("db.save(user);", "src/Component.test-d.tsx").is_empty());
    }

    #[test]
    fn still_flags_floating_call_in_regular_file() {
        // Control: the same statement still fires outside a `.test-d.` file.
        let d = run_at("db.save(user);", "src/index.ts");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #1636: a promise-returning call that is the
    // concise (expression) body of an arrow function is the function's implicit
    // return value, not a discarded statement — it must not be flagged.

    #[test]
    fn allows_promise_combinator_in_arrow_concise_body() {
        assert!(run_on("page.evaluate(value => Promise.resolve(value), null);").is_empty());
    }

    #[test]
    fn allows_member_call_in_arrow_concise_body() {
        // `repo.save(item)` is the concise body of the `.map` callback — its
        // promise is collected by `map`, not floated.
        assert!(
            run_on("await Promise.all(items.map(item => repo.save(item)));").is_empty()
        );
    }

    #[test]
    fn allows_promise_reject_in_async_arrow_concise_body() {
        assert!(
            run_on("const fail = async () => Promise.reject(new Error('error'));").is_empty()
        );
    }

    #[test]
    fn still_flags_floating_call_in_arrow_block_body() {
        // Negative-space guard: a promise-returning call as a discarded statement
        // inside an arrow's *block* body (not the concise body) still fires.
        let d = run_on("const run = () => { db.save(user); };");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #1649: Web Audio's
    // `AudioNode.prototype.connect(destination)` returns the destination AudioNode
    // (for chaining) or void — never a Promise — so chained `.connect().connect()`
    // calls and `.connect(ctx.destination)` calls must not be flagged.

    #[test]
    fn allows_audio_node_connect_to_destination() {
        let src = "\
let ctx = new AudioContext();
let masterGain = ctx.createGain();
masterGain.connect(ctx.destination);
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chained_audio_node_connect() {
        let src = "\
osc.connect(gain).connect(masterGain);
noise.connect(band).connect(noiseGain).connect(masterGain);
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_non_audio_connect() {
        // Negative-space guard: a genuine Promise-returning `.connect()` (e.g. a
        // DB/socket client) on a plain receiver with no Web Audio signal stays
        // flagged.
        let d = run_on("client.connect(connectionString);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_floating_async_call() {
        // Negative-space guard: a discarded async-function result still fires.
        let d = run_on("repo.save(entity);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #3377: `.commit()` and `.flush()` are dominated
    // by synchronous APIs (data-loader state staging, transaction commits, buffer
    // / scheduler / resolver draining), so both names were dropped from the
    // heuristic. A statement-level `.commit(...)` / `.flush()` must not be flagged.

    #[test]
    fn allows_void_commit_call() {
        // Vue Router data-loader `entry.commit(to)` — a synchronous void commit.
        let src = "\
entry.commit(to);
childEntry.commit(to);
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_flush_call() {
        // Vue Router e2e `scrollWaiter.flush()` — a synchronous void resolver, on a
        // receiver that does not read as a test scheduler.
        assert!(run_on("scrollWaiter.flush();").is_empty());
    }

    #[test]
    fn still_flags_genuine_async_save_after_commit_flush_drop() {
        // Over-exemption guard: dropping `commit`/`flush` must not weaken the
        // strong async signals — a discarded `.save(...)` still fires.
        let d = run_on("repo.save(entity);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #2391: better-sqlite3 is a fully synchronous
    // library — its `Statement.run()` and `Database.exec()` return non-Promise
    // values. When a file imports `better-sqlite3`, these statement-level calls
    // must not be flagged.

    #[test]
    fn allows_better_sqlite3_sync_methods() {
        // The issue's exact examples from prisma's better-sqlite3 adapter.
        let src = "\
import Database from 'better-sqlite3';

class Adapter {
  executeScript(script: string): Promise<void> {
    this.client.exec(script);
    return Promise.resolve();
  }

  async startTransaction(): Promise<void> {
    this.client.prepare('BEGIN').run();
  }

  runStatement(stmt: Statement): void {
    stmt.run();
  }
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_floating_promise_without_better_sqlite3_import() {
        // Negative-space guard: the same `.run()` / `.exec()` names stay flagged
        // in a file that does not import better-sqlite3.
        let d = run_on("job.run();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_genuine_floating_promise_in_better_sqlite3_file() {
        // Negative-space guard: only the better-sqlite3 synchronous method names
        // (`run`, `exec`) are exempted — a genuine Promise-returning call (e.g.
        // `repo.save(...)`) in the same file stays flagged.
        let src = "\
import Database from 'better-sqlite3';

repo.save(entity);
";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #2364: Express's `Route.dispatch(req, res, done)`
    // is continuation-passing — it delivers its outcome through the trailing `done`
    // callback and returns void, never a Promise. A `.dispatch(...)` call with the
    // Express dispatch shape (three args, trailing callback) must not be flagged.
    // (`res.send()` / `res.json()` were already exempt — `send`/`json` are not in
    // the heuristic list.)

    #[test]
    fn allows_express_route_dispatch_identifier_callback() {
        // The issue's exact example: `route.dispatch(req, {}, done)`.
        assert!(run_on("route.dispatch(req, {}, done);").is_empty());
    }

    #[test]
    fn allows_express_route_dispatch_arrow_callback() {
        assert!(run_on("route.dispatch(req, {}, (err) => { if (err) throw err; });").is_empty());
    }

    #[test]
    fn allows_express_route_dispatch_function_callback() {
        let src = "\
route.dispatch(req, {}, function (err) {
  assert.ifError(err);
});
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_dispatch_without_trailing_callback() {
        // Negative-space guard: a `.dispatch(...)` with three non-callback args is
        // not the Express dispatch shape and stays flagged.
        let d = run_on("worker.dispatch(a, b, c);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_single_arg_dispatch() {
        // Negative-space guard: a genuine Promise-returning `.dispatch(job)` on a
        // non-store, non-view receiver stays flagged.
        let d = run_on("worker.dispatch(job);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #2364: synchronous Express `ServerResponse`
    // methods (`res.send` / `res.json`) return the response for chaining, not a
    // Promise. These names are not in the heuristic list, so they are not flagged.

    #[test]
    fn allows_express_res_send() {
        let src = "app.get('/', (req, res) => { res.send('ok'); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_express_res_status_send() {
        assert!(run_on("res.status(500).send('got error');").is_empty());
    }

    #[test]
    fn allows_express_res_json() {
        assert!(run_on("res.json(data);").is_empty());
    }

    // Regression tests for issue #2284: the Three.js / react-three-fiber
    // ecosystem reuses `connect` / `save` / `load` for synchronous, non-Promise
    // operations (controls DOM wiring, controls state snapshot/restore,
    // `CanvasRenderingContext2D.save()`). In a file that imports `three` /
    // `@react-three/*`, these statement-level calls must not be flagged.
    // (`update` was already dropped from the heuristic in #1280.)

    #[test]
    fn allows_threejs_controls_connect() {
        // The issue's exact example from drei's OrbitControls.tsx.
        let src = "\
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls';
controls.connect(explDomElement);
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_threejs_canvas_context_save() {
        // The issue's exact example from drei's GradientTexture.tsx.
        let src = "\
import * as THREE from 'three';
context.save();
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_react_three_fiber_controls_load() {
        let src = "\
import { useFrame } from '@react-three/fiber';
controls.load();
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_save_without_threejs_import() {
        // Negative-space guard: ORM `.save()` (TypeORM/Mongoose/Sequelize) returns
        // a Promise — floating it is a real bug — so `.save()` stays flagged in a
        // file with no Three.js import.
        let d = run_on("userRepository.save(user);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_load_without_threejs_import() {
        let d = run_on("repo.load(id);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_connect_without_threejs_import() {
        // A genuine Promise-returning DB/socket client `.connect()` stays flagged.
        let d = run_on("client.connect(connectionString);");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #1280: Angular's `WritableSignal.update(updater)`
    // synchronously updates the signal value and returns void. `update` is a very
    // common synchronous mutation name (Angular signals, Immutable.js, stores,
    // Map-likes), so it is not in the heuristic list and must not be flagged.

    #[test]
    fn allows_angular_signal_update() {
        // The issue's exact example: `this.page.update((c) => ...)` as a statement
        // inside an Angular component method.
        let src = "\
class ExampleComponent {
  readonly page = signal(0);
  previousPage() {
    this.page.update((currentPage) => {
      return Math.max(currentPage - 1, 0);
    });
  }
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_update_call() {
        assert!(run_on("store.update(state);").is_empty());
    }

    #[test]
    fn still_flags_genuine_floating_promise_after_update_removed() {
        // Negative-space guard: another async-dominant method name still in the
        // heuristic (`fetch`) used as a statement stays flagged.
        let d = run_on("api.fetch(url);");
        assert_eq!(d.len(), 1);
    }

    // Regression test for issue #2116: the `.sync` suffix is the synchronous
    // counterpart of an async API (`execa.sync()`), returns a plain value, and
    // must not be flagged.

    #[test]
    fn allows_execa_sync() {
        let src = "\
execa.sync('yarn', ['link', '--private', '--all', rootDirectory], {
  cwd,
  stdio: 'inherit',
});
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_genuine_floating_promise_after_sync_removed() {
        // Negative-space guard: an async-dominant method name still in the
        // heuristic (`fetch`) used as a statement stays flagged.
        let d = run_on("api.fetch(url);");
        assert_eq!(d.len(), 1);
    }
}

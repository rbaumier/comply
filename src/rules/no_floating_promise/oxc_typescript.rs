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

        if is_test_scheduler_flush(node, call, semantic) {
            return;
        }

        let is_flag = is_promise_combinator(call) || is_async_looking_member_call(call);
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
fn is_async_looking_member_call(call: &CallExpression) -> bool {
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
    let method = member.property.name.as_str();
    ASYNC_LOOKING_METHODS.contains(&method)
}

/// RxJS `TestScheduler.prototype.flush()` runs all scheduled virtual-time
/// actions synchronously and returns `void` per `rxjs/testing` — there is no
/// Promise to await. Matches an argument-less `.flush()` (the `flush(): void`
/// signature) by either type-free syntactic signal:
///   - the receiver is a bare identifier whose name (case-insensitive) reads as
///     a test scheduler, e.g. `scheduler.flush()`, `testScheduler.flush()`; or
///   - the call is lexically inside a jasmine-marbles `marbles(...)` callback,
///     where the receiver is the callback's marble helper, e.g.
///     `marbles((m) => { …; m.flush(); })`.
fn is_test_scheduler_flush(
    node: &oxc_semantic::AstNode,
    call: &CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "flush" || !call.arguments.is_empty() {
        return false;
    }
    if matches!(&member.object, Expression::Identifier(id) if id.name.as_str().to_lowercase().contains("scheduler"))
    {
        return true;
    }
    is_inside_marbles_callback(node, semantic)
}

/// True when `node` is nested inside a call to `marbles(...)` — the jasmine-marbles
/// helper that supplies the RxJS marble-test scheduler to its callback.
fn is_inside_marbles_callback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    semantic.nodes().ancestors(node.id()).any(|ancestor| {
        matches!(ancestor.kind(), AstKind::CallExpression(call)
            if matches!(&call.callee, Expression::Identifier(id) if id.name.as_str() == "marbles"))
    })
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

    // Regression tests for issue #1605: RxJS `TestScheduler.flush()` runs all
    // scheduled virtual-time actions synchronously and returns void — there is
    // no Promise to await — so an argument-less `.flush()` on a scheduler-named
    // receiver, or inside a jasmine-marbles `marbles(...)` callback, must not be
    // flagged.

    #[test]
    fn allows_marbles_callback_flush() {
        // The issue's exact example: `m.flush()` inside a `marbles((m) => {...})`
        // callback, where `m` is the marble-test scheduler helper.
        let src = "\
it('(Marbles) should complete the effect', marbles((m) => {
  updater(UPDATED);
  m.flush();
  updater(UPDATE_VALUE);
  m.flush();
}));
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_named_test_scheduler_flush() {
        let src = "\
const testScheduler = new TestScheduler(assertDeepEqual);
testScheduler.flush();
scheduler.flush();
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_non_scheduler_flush() {
        // Negative-space guard: a genuine Promise-returning `.flush()` (e.g. a
        // buffered DB/IO writer) on a plain receiver outside any marbles callback
        // stays flagged.
        let d = run_on("writer.flush();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_flush_with_argument() {
        // Negative-space guard: `.flush(...)` with an argument is not the
        // `flush(): void` scheduler signature, so it stays flagged.
        let d = run_on("buffer.flush(chunk);");
        assert_eq!(d.len(), 1);
    }
}

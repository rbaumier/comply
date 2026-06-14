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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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
        let Expression::CallExpression(call) = &stmt.expression else {
            return;
        };

        // Check if already handled by .then/.catch/.finally
        if has_promise_handler(call) {
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
    if is_process_std_stream_write(member) {
        return false;
    }
    if is_stream_controller_close(member) {
        return false;
    }
    if is_xml_http_request_send(member) {
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
    if is_event_emitter_emit(member) {
        return false;
    }
    let method = member.property.name.as_str();
    ASYNC_LOOKING_METHODS.contains(&method)
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

/// Node's `EventEmitter.prototype.emit(event, ...args)` returns `boolean`
/// (whether any listener fired), not a Promise — there is nothing to await.
/// Matches `.emit(...)` whose receiver is `this` (a class extending
/// `EventEmitter`, e.g. tiptap's `Editor.emit(...)`) or a bare identifier whose
/// name (case-insensitive) reads as an event emitter, e.g. `emitter`,
/// `eventEmitter`, `eventBus`, `bus`.
fn is_event_emitter_emit(member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "emit" {
        return false;
    }
    match &member.object {
        Expression::ThisExpression(_) => true,
        Expression::Identifier(id) => {
            let name = id.name.as_str().to_lowercase();
            name.contains("emitter") || name == "eventbus" || name == "bus"
        }
        _ => false,
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

/// `XMLHttpRequest.prototype.send(...)` returns `void` per `lib.dom.d.ts` — the
/// legacy XHR API delivers its result through the `onreadystatechange` callback,
/// not a Promise, so there is nothing to await.
/// Matches when the receiver is a bare identifier whose name (case-insensitive)
/// reads as an XHR handle, e.g. `xmlHttp`, `xhr`, `xmlHttpRequest`.
fn is_xml_http_request_send(member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "send" {
        return false;
    }
    let Expression::Identifier(id) = &member.object else {
        return false;
    };
    let name = id.name.as_str().to_lowercase();
    name.contains("xhr") || name.contains("xmlhttp")
}

/// Redux's `Store.dispatch(action)` returns the dispatched `Action` synchronously
/// for a plain action object — there is no Promise to await or catch.
/// Matches when the `.dispatch(...)` receiver is a bare identifier whose name
/// (case-insensitive) reads as a Redux store, e.g. `store`, `reduxStore`,
/// `appStore`.
fn is_redux_store_dispatch(member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "dispatch" {
        return false;
    }
    matches!(&member.object, Expression::Identifier(id) if id.name.as_str().to_lowercase().contains("store"))
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

/// `ReadableStreamDefaultController.close()` / `WritableStreamDefaultController.close()` etc.
/// return `void` per the WHATWG Streams spec — nothing to await.
/// Matches when the receiver is a bare identifier whose name contains "controller".
fn is_stream_controller_close(member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "close" {
        return false;
    }
    matches!(&member.object, Expression::Identifier(id) if id.name.as_str().to_lowercase().contains("controller"))
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

/// `process.stdout.write(...)` / `process.stderr.write(...)` return `boolean`
/// (the backpressure signal), not a Promise — there is nothing to await.
fn is_process_std_stream_write(member: &StaticMemberExpression) -> bool {
    if member.property.name.as_str() != "write" {
        return false;
    }
    let Expression::StaticMemberExpression(stream) = &member.object else {
        return false;
    };
    if !matches!(stream.property.name.as_str(), "stdout" | "stderr") {
        return false;
    }
    matches!(&stream.object, Expression::Identifier(id) if id.name.as_str() == "process")
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

    // Regression for #291: process.stdout/stderr.write() return `boolean`
    // (backpressure), not a Promise — nothing to await.
    #[test]
    fn allows_process_stderr_write() {
        assert!(run_on("process.stderr.write(\"oops\\n\");").is_empty());
    }

    #[test]
    fn allows_process_stdout_write() {
        assert!(run_on("process.stdout.write(`${label} done\\n`);").is_empty());
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

    // Regression tests for issue #758: ReadableStreamDefaultController.close() returns void,
    // not a Promise — it must not be flagged.

    #[test]
    fn allows_stream_controller_close() {
        let src = "\
async function pull(controller) {
  const next = await someGenerator.next();
  if (next.done) {
    controller.close();
  }
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_non_controller_close() {
        let d = run_on("db.close();");
        assert_eq!(d.len(), 1);
    }

    // Regression tests for issue #1104: XMLHttpRequest.send() returns void per
    // lib.dom.d.ts — the legacy XHR callback API, not a Promise — so it must not
    // be flagged.

    #[test]
    fn allows_xml_http_request_send() {
        let src = "\
function httpGetAsync(targetUrl, callback) {
  var xmlHttp = new XMLHttpRequest();
  xmlHttp.onreadystatechange = function () {
    if (xmlHttp.readyState == 4 && xmlHttp.status == 200)
      callback(xmlHttp.responseText);
  }
  xmlHttp.open(\"GET\", targetUrl, true);
  xmlHttp.send(null);
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_xhr_send() {
        assert!(run_on("xhr.send(body);").is_empty());
    }

    #[test]
    fn still_flags_non_xhr_send() {
        let d = run_on("producer.send(message);");
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

    // Regression tests for issue #1819: Node's `EventEmitter.prototype.emit(...)`
    // returns `boolean`, not a Promise — tiptap's `Editor` extends an EventEmitter
    // and calls `this.emit(...)` for lifecycle hooks, so a `.emit(...)` on `this`
    // or an emitter-named receiver must not be flagged.

    #[test]
    fn allows_this_emit() {
        let src = "\
this.emit('beforeCreate', { editor: this })
this.emit('mount', { editor: this })
this.emit('create', { editor: this })
this.emit('unmount', { editor: this })
this.emit('update', { editor: this, transaction: this.state.tr, appendedTransactions: [] })
this.emit('selectionUpdate', { editor: this, transaction: this.state.tr })
this.emit('transaction', { editor: this, transaction: this.state.tr, appendedTransactions: [] })
this.emit('focus', { editor: this, event: view.dom, transaction })
this.emit('blur', { editor: this, event: view.dom, transaction })
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_named_emitter_emit() {
        let src = "\
emitter.emit('done', payload);
eventBus.emit('change', value);
bus.emit('tick');
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_non_emitter_emit() {
        // Control: `.emit(...)` on a non-emitter receiver stays flagged —
        // e.g. a producer that returns a Promise.
        let d = run_on("producer.emit(record);");
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
}

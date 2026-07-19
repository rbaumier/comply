use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::GetSpan;
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // `*.test-d.{ts,tsx}` are tsd / `expect-type` type-declaration tests:
        // call statements there are type assertions checked by `tsc --noEmit`,
        // never executed, so a "floating" promise can never reject at runtime.
        if crate::rules::path_utils::has_test_d_infix(ctx.path) {
            return Vec::new();
        }

        let evidence = AsyncEvidence::collect(semantic, ctx.source);
        let mut diagnostics = Vec::new();

        for node in semantic.nodes() {
            let AstKind::ExpressionStatement(stmt) = node.kind() else {
                continue;
            };
            // OXC normalises an arrow's concise body (`() => expr`) into a
            // synthetic ExpressionStatement under an ArrowFunctionExpression with
            // `expression == true`. Such a node is the function's implicit return
            // value, not a discarded statement, so a promise it produces is handed
            // back to the caller, not floated.
            if is_concise_arrow_body(node, semantic) {
                continue;
            }
            let Expression::CallExpression(call) = &stmt.expression else {
                continue;
            };
            // Already handled by `.then`/`.catch`/`.finally`.
            if has_promise_handler(call) {
                continue;
            }
            if is_promise_combinator(call)
                || evidence.call_is_promise(call, node.id(), ctx.source, semantic)
            {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Promise-returning call is used as a statement \u{2014} rejections will \
                              become UnhandledPromiseRejection. Add `await`, chain `.catch`, \
                              or prefix with `void` if you really want to ignore it."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

/// Real, in-file evidence that a given call returns a Promise — gathered once per
/// file in a single semantic walk. The rule fires only against this evidence,
/// never against the method name, so a synchronous chainable method that merely
/// shares a name with an async API (pdfkit's `doc.save()`, a sync `.run()`, ...)
/// is never flagged.
struct AsyncEvidence {
    /// Receiver-method shapes (`"<receiver-text>.<method>"`) that are `await`ed or
    /// `.then`/`.catch`/`.finally`-chained somewhere in the file, for receivers
    /// that are NOT rooted at `this`. Seeing the same shape used as an
    /// awaited/handled promise proves that method returns a Promise on that
    /// receiver, so an un-awaited sibling call genuinely floats.
    awaited_member_shapes: HashSet<String>,
    /// Awaited/handled `this.`-rooted shapes, each paired with the `NodeId` of
    /// its nearest enclosing class. `this` means a different receiver in each
    /// class, so keying by receiver text alone would let one class's awaited
    /// evidence match a sibling class's identically-named call; scoping to the
    /// enclosing class keeps sibling classes distinct — mirroring the per-symbol
    /// scoping already applied to bare async-function calls.
    awaited_this_shapes: HashSet<(NodeId, String)>,
    /// `SymbolId`s of functions declared `async` in this file (`async function f`
    /// / `const f = async () => ...`). A bare `f()` statement whose callee
    /// reference resolves to one of these symbols floats the returned promise.
    /// Resolving by symbol — not by name — keeps two same-named inner functions in
    /// different scopes (one async, one sync) distinct.
    async_function_symbols: HashSet<SymbolId>,
}

impl AsyncEvidence {
    fn collect(semantic: &oxc_semantic::Semantic, source: &str) -> Self {
        let mut awaited_member_shapes = HashSet::new();
        let mut awaited_this_shapes = HashSet::new();
        let mut async_function_symbols = HashSet::new();

        for node in semantic.nodes() {
            match node.kind() {
                // `await <expr>` — record the awaited call's receiver-method shape.
                AstKind::AwaitExpression(await_expr) => {
                    if let Expression::CallExpression(call) = &await_expr.argument
                        && let Some(shape) = member_call_shape(call, source)
                    {
                        match classify_member_shape(call, shape, node.id(), semantic) {
                            ShapeEvidence::ScopedToClass(class_id, shape) => {
                                awaited_this_shapes.insert((class_id, shape));
                            }
                            ShapeEvidence::ByText(shape) => {
                                awaited_member_shapes.insert(shape);
                            }
                        }
                    }
                }
                // `<expr>.then(...)` / `.catch(...)` / `.finally(...)` — the inner
                // receiver is a promise. Record the inner call's shape.
                AstKind::CallExpression(call) => {
                    if let Some(inner) = promise_handler_inner_call(call)
                        && let Some(shape) = member_call_shape(inner, source)
                    {
                        match classify_member_shape(inner, shape, node.id(), semantic) {
                            ShapeEvidence::ScopedToClass(class_id, shape) => {
                                awaited_this_shapes.insert((class_id, shape));
                            }
                            ShapeEvidence::ByText(shape) => {
                                awaited_member_shapes.insert(shape);
                            }
                        }
                    }
                }
                AstKind::Function(func) => {
                    if func.r#async
                        && let Some(id) = &func.id
                        && let Some(symbol_id) = id.symbol_id.get()
                    {
                        async_function_symbols.insert(symbol_id);
                    }
                }
                AstKind::VariableDeclarator(declarator) => {
                    if let Some(symbol_id) = async_initializer_binding(declarator) {
                        async_function_symbols.insert(symbol_id);
                    }
                }
                _ => {}
            }
        }

        Self {
            awaited_member_shapes,
            awaited_this_shapes,
            async_function_symbols,
        }
    }

    /// True when `call` has real evidence of returning a Promise: it is a bare
    /// call whose callee reference resolves to a locally-declared `async` function
    /// symbol, or a `receiver.method(...)` whose same shape is awaited /
    /// promise-handled elsewhere in the file. `stmt_node_id` is the id of the
    /// statement node holding `call`, used to resolve the enclosing class when the
    /// receiver is rooted at `this`.
    fn call_is_promise(
        &self,
        call: &CallExpression,
        stmt_node_id: NodeId,
        source: &str,
        semantic: &oxc_semantic::Semantic,
    ) -> bool {
        match &call.callee {
            Expression::Identifier(id) => id
                .reference_id
                .get()
                .and_then(|ref_id| semantic.scoping().get_reference(ref_id).symbol_id())
                .is_some_and(|symbol_id| self.async_function_symbols.contains(&symbol_id)),
            Expression::StaticMemberExpression(_) => {
                let Some(shape) = member_call_shape(call, source) else {
                    return false;
                };
                match classify_member_shape(call, shape, stmt_node_id, semantic) {
                    ShapeEvidence::ScopedToClass(class_id, shape) => {
                        self.awaited_this_shapes.contains(&(class_id, shape))
                    }
                    ShapeEvidence::ByText(shape) => self.awaited_member_shapes.contains(&shape),
                }
            }
            _ => false,
        }
    }
}

/// `"<receiver-text>.<method>"` for a static-member call (`db.users.save(x)` ->
/// `"db.users.save"`), or `None` when the callee is not a static member access.
fn member_call_shape(call: &CallExpression, source: &str) -> Option<String> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let object = peel_parens(&member.object);
    let obj_span = object.span();
    let obj_text = &source[obj_span.start as usize..obj_span.end as usize];
    Some(format!("{obj_text}.{}", member.property.name.as_str()))
}

/// Which evidence bucket a member-call `shape` belongs to. `this` denotes a
/// different receiver in each class, so a `this.`-rooted shape is scoped to the
/// id of its nearest enclosing class; every other receiver keeps its plain text
/// key. Both collection and lookup route through here, so the two sides always
/// agree on the bucket.
enum ShapeEvidence {
    /// A `this.`-rooted shape, scoped to its nearest enclosing class node.
    ScopedToClass(NodeId, String),
    /// Any non-`this` receiver (or a `this.` shape with no enclosing class),
    /// keyed by receiver text alone.
    ByText(String),
}

/// Classify `shape` for `call` located under `node_id`: a `this.`-rooted call
/// with an enclosing class is scoped to that class; anything else falls back to
/// the text key.
fn classify_member_shape(
    call: &CallExpression,
    shape: String,
    node_id: NodeId,
    semantic: &oxc_semantic::Semantic,
) -> ShapeEvidence {
    match member_call_receiver_is_this(call)
        .then(|| nearest_enclosing_class_id(node_id, semantic))
        .flatten()
    {
        Some(class_id) => ShapeEvidence::ScopedToClass(class_id, shape),
        None => ShapeEvidence::ByText(shape),
    }
}

/// True when the root receiver of a static-member call is `this` — i.e. the
/// shape is anchored to `this` (`this.m()`, `this.p.m()`, `this.p[i].m()`, ...).
fn member_call_receiver_is_this(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let mut current = peel_parens(&member.object);
    loop {
        match current {
            Expression::ThisExpression(_) => return true,
            Expression::StaticMemberExpression(m) => current = peel_parens(&m.object),
            Expression::ComputedMemberExpression(m) => current = peel_parens(&m.object),
            _ => return false,
        }
    }
}

/// `NodeId` of the nearest enclosing `class` for `node_id`, or `None` when the
/// node is not inside a class.
fn nearest_enclosing_class_id(
    node_id: NodeId,
    semantic: &oxc_semantic::Semantic,
) -> Option<NodeId> {
    semantic
        .nodes()
        .ancestors(node_id)
        .find(|ancestor| matches!(ancestor.kind(), AstKind::Class(_)))
        .map(|ancestor| ancestor.id())
}

/// When `call` is `<inner>.then(...)` / `.catch(...)` / `.finally(...)`, return
/// the `<inner>` call expression (the promise being handled), else `None`.
fn promise_handler_inner_call<'a>(call: &'a CallExpression<'a>) -> Option<&'a CallExpression<'a>> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if !matches!(member.property.name.as_str(), "then" | "catch" | "finally") {
        return None;
    }
    match peel_parens(&member.object) {
        Expression::CallExpression(inner) => Some(inner),
        _ => None,
    }
}

/// The bound symbol when a variable declarator initializes to an `async` function
/// or arrow expression (`const f = async () => ...`), else `None`.
fn async_initializer_binding(declarator: &VariableDeclarator) -> Option<SymbolId> {
    let BindingPattern::BindingIdentifier(ident) = &declarator.id else {
        return None;
    };
    let is_async = match declarator.init.as_ref()? {
        Expression::ArrowFunctionExpression(arrow) => arrow.r#async,
        Expression::FunctionExpression(func) => func.r#async,
        _ => false,
    };
    is_async.then(|| ident.symbol_id.get()).flatten()
}

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
    matches!(member.property.name.as_str(), "then" | "catch" | "finally")
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

/// Unwrap any `ParenthesizedExpression` wrappers around `expr`.
fn peel_parens<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    while let Expression::ParenthesizedExpression(p) = current {
        current = &p.expression;
    }
    current
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

    // --- Promise combinators: always a Promise, no extra evidence needed ---

    #[test]
    fn flags_floating_promise_all() {
        assert_eq!(run_on("Promise.all([a, b]);").len(), 1);
    }

    #[test]
    fn allows_promise_combinator_with_then() {
        assert!(run_on("Promise.all([a, b]).then(done);").is_empty());
    }

    // --- Evidence: same receiver-method awaited elsewhere in the file ---

    #[test]
    fn flags_floating_call_when_same_shape_awaited_elsewhere() {
        // `repo.save(...)` is awaited once and floated once — the floating one is a
        // genuine bug, proven by the awaited sibling.
        let src = "\
async function run() {
  await repo.save(a);
  repo.save(b);
}
";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_floating_call_when_same_shape_then_chained_elsewhere() {
        let src = "\
api.fetch(url).then(handle);
api.fetch(other);
";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_then_chain_self() {
        assert!(run_on("api.fetch(url).then(handleResult);").is_empty());
    }

    // --- Evidence: bare call to a locally-declared async function ---

    #[test]
    fn flags_floating_call_to_local_async_function() {
        let src = "\
async function sync() { await doWork(); }
sync();
";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_floating_call_to_local_async_arrow() {
        let src = "\
const sync = async () => doWork();
sync();
";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_awaited_call_to_local_async_function() {
        let src = "\
async function sync() { await doWork(); }
async function main() { await sync(); }
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_sync_call_when_same_name_async_in_other_scope() {
        // Issue #7108: `slice` inside `parseSync` is a *sync* inner function; the
        // async `slice` lives in the unrelated `parse` scope. The callee resolves
        // by symbol to the sync declaration, so `slice(2)` must not be flagged.
        let src = "\
async function parse(md) {
  async function slice(end) { await load(end); }
  await slice(1);
}
function parseSync(md) {
  function slice(end) { return end; }
  slice(2);
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_floating_async_call_despite_same_name_sync_sibling() {
        // The async `slice` floated inside `parse` (no await) still flags; the
        // same-named sync `slice` in `parseSync` does not — one diagnostic total.
        let src = "\
async function parse(md) {
  async function slice(end) { await load(end); }
  slice(1);
}
function parseSync(md) {
  function slice(end) { return end; }
  slice(2);
}
";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn shadowing_resolves_per_scope() {
        // Outer async `f` shadowed by an inner sync `f`: the inner call resolves
        // to the sync symbol (not flagged), the outer call to the async symbol
        // (flagged) — one diagnostic total.
        let src = "\
async function f() { await work(); }
function outer() {
  function f() { return 1; }
  f();
}
f();
";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- Core FP: a method that merely shares an async-sounding name but has no
    // Promise evidence is never flagged. ---

    #[test]
    fn allows_pdfkit_doc_save() {
        // Issue #5323: pdfkit's `doc.save()` is the synchronous PDF `q` graphics
        // operator (returns `this`); it is never awaited, so no evidence exists.
        let src = "\
doc.save();
doc.restore();
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pdfkit_this_save() {
        // `lib/mixins/text.js:78` — `this.save()` inside a pdfkit mixin.
        let src = "\
class PDFDocument {
  addLine() {
    this.save();
  }
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_db_save_without_evidence() {
        // A bare `db.save(user)` with no awaited sibling and no local async decl
        // carries no Promise evidence, so the name `save` alone never fires.
        assert!(run_on("db.save(user);").is_empty());
    }

    #[test]
    fn allows_canvas_context_save() {
        // `CanvasRenderingContext2D.save()` — synchronous graphics-state push.
        assert!(run_on("context.save();").is_empty());
    }

    #[test]
    fn allows_better_sqlite3_run_without_evidence() {
        // better-sqlite3 is fully synchronous; its `.run()` is never awaited, so
        // no evidence is recorded and the call is not flagged — without any
        // library-specific carve-out.
        let src = "\
this.client.exec(script);
stmt.run();
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_dispatch_without_evidence() {
        // Redux/NgRx `.dispatch(...)` is synchronous; never awaited.
        assert!(run_on("store.dispatch(action);").is_empty());
    }

    #[test]
    fn allows_audio_node_connect() {
        // Web Audio `.connect(...)` returns the node; never awaited.
        assert!(run_on("masterGain.connect(ctx.destination);").is_empty());
    }

    #[test]
    fn allows_tiptap_chain_run() {
        // tiptap fluent builder `.run()` returns boolean; never awaited.
        assert!(run_on("editor.chain().focus().toggleBold().run();").is_empty());
    }

    // --- Receiver text disambiguation: the shape includes the receiver, so an
    // awaited `db.save` does not exempt/implicate an unrelated `doc.save`. ---

    #[test]
    fn evidence_is_receiver_specific() {
        // `db.save(...)` is awaited (so `db.save` floats elsewhere), but
        // `doc.save(...)` has its own receiver and no evidence — only the `db`
        // floating call fires.
        let src = "\
async function run() {
  await db.save(a);
  db.save(b);
  doc.save();
}
";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- `this.`-rooted shapes are scoped to their enclosing class, so sibling
    // classes sharing a method name do not leak awaited evidence to each other ---

    #[test]
    fn allows_sync_this_calls_when_async_sibling_class_shares_method_names() {
        // Issue #7797: two classes in one file share method names on a `this.`
        // receiver — one sync (`: void`), one async. The async class awaits its
        // own methods; that evidence must not leak across the class boundary and
        // implicate the sync class's identically-named `this.` calls.
        let src = "\
class SyncCrawler {
  crawlObject(): void {
    this.crawlObject();
    this.helper();
  }
  helper(): void {}
}
class AsyncCrawler {
  async crawlObject(): Promise<void> {
    await this.crawlObject();
    await this.helper();
  }
  async helper(): Promise<void> {}
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_floating_this_call_in_async_class_despite_sync_sibling() {
        // The async class awaits `this.helper()` once (proving it returns a
        // Promise in *this* class) and floats it once — the floating call still
        // flags. The sync sibling sharing the name is untouched: one diagnostic.
        let src = "\
class SyncCrawler {
  crawlObject(): void {
    this.helper();
  }
  helper(): void {}
}
class AsyncCrawler {
  async crawlObject(): Promise<void> {
    await this.helper();
    this.helper();
  }
  async helper(): Promise<void> {}
}
";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_floating_this_call_matching_same_class_awaited_evidence() {
        // Class-scoping must not be too coarse: within one class, `this.load()`
        // is awaited once and floated once — the same-class evidence still
        // matches the floating sibling.
        let src = "\
class Repo {
  async sync(): Promise<void> {
    await this.load();
    this.load();
  }
  async load(): Promise<void> {}
}
";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- `.test-d.` type-declaration tests are never flagged ---

    #[test]
    fn allows_floating_call_in_test_d_ts() {
        let src = "\
async function run() {
  await repo.save(a);
}
repo.save(b);
";
        assert!(run_at(src, "src/Component.test-d.ts").is_empty());
    }

    // --- Concise arrow body is an implicit return, not a floated statement ---

    #[test]
    fn allows_promise_combinator_in_arrow_concise_body() {
        assert!(run_on("page.evaluate(value => Promise.resolve(value), null);").is_empty());
    }

    #[test]
    fn allows_member_call_in_arrow_concise_body() {
        // `repo.save(item)` is the concise body of the `.map` callback — its
        // promise is collected by `map`, not floated — even though `repo.save` is
        // awaited elsewhere.
        let src = "\
async function run() {
  await repo.save(first);
  await Promise.all(items.map(item => repo.save(item)));
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_floating_call_in_arrow_block_body() {
        // Negative-space guard: a promise-returning call as a discarded statement
        // inside an arrow's *block* body (not the concise body) still fires.
        let src = "\
async function run() {
  await db.save(a);
}
const go = () => { db.save(b); };
";
        assert_eq!(run_on(src).len(), 1);
    }
}

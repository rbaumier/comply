//! no-async-without-await OXC backend — flag `async` functions that contain
//! no `await` or `for await` in their own body.

use rustc_hash::FxHashSet;
use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{ClassShape, byte_offset_to_line_col, enclosing_class};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_test_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("/tests/")
        || s.contains("\\tests\\")
}

/// Check if a function node has an explicit Promise return type annotation.
fn has_promise_return_type(
    source: &str,
    return_type: &Option<oxc_allocator::Box<oxc_ast::ast::TSTypeAnnotation>>,
) -> bool {
    let Some(rt) = return_type else { return false };
    let text = &source[rt.span.start as usize..rt.span.end as usize];
    text.contains("Promise<") || text.contains("PromiseLike<")
}

/// Find the nearest enclosing async function/arrow for a given node,
/// stopping at function boundaries. Returns the NodeId of the nearest
/// enclosing function/arrow.
fn nearest_function_id(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return Some(ancestor.id());
            }
            _ => {}
        }
    }
    None
}

/// Check if the function/arrow node is passed as an argument to a call
/// expression (i.e. it is a callback). In oxc's semantic tree, arguments have
/// no wrapper node, so a callback's immediate parent is the `CallExpression`
/// itself. The callee position (an IIFE like `(async () => {})()`) is excluded
/// by requiring the node to appear in the call's `arguments`.
fn is_call_argument(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let parent = semantic.nodes().parent_node(func_node.id());
    let AstKind::CallExpression(call) = parent.kind() else { return false };
    let span = func_node.kind().span();
    call.arguments
        .iter()
        .any(|arg| arg.span() == span)
}

/// Check if the function/arrow node is the value of a JSX attribute, e.g.
/// `<form action={async () => {}}>`. The parent chain is `Function ->
/// JSXExpressionContainer -> JSXAttribute`. Like a bare call-argument callback,
/// the attribute's type contract owns the signature: JSX props such as the
/// Next.js App Router `<form action>` are typed `() => Promise<void>`, so `async`
/// is mandatory even when the body fires a bound action without awaiting it. The
/// author does not control the call site, so the missing `await` is not a smell.
fn is_jsx_attribute_value(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let container = nodes.parent_node(func_node.id());
    let AstKind::JSXExpressionContainer(_) = container.kind() else {
        return false;
    };
    matches!(
        nodes.parent_node(container.id()).kind(),
        AstKind::JSXAttribute(_)
    )
}

/// Check if the async function is a property of an object literal that is passed
/// as an argument to a call expression, covering both the shorthand-method shape
/// (`$config({ async run() {} })`) and the arrow-value shape
/// (`useForm({ onSubmit: async () => {} })`). The walk is `Function ->
/// ObjectProperty -> ObjectExpression -> CallExpression(arguments)`. Like a bare
/// arrow callback, the callee owns the contract: framework/library options
/// objects type such callbacks `(...) => Promise<T>` (SST/Pulumi `run()`,
/// TanStack Form `onSubmit`), so `async` is mandatory even when the body never
/// awaits.
fn is_object_property_in_call_arg(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();

    let property = nodes.parent_node(func_node.id());
    let AstKind::ObjectProperty(_) = property.kind() else { return false };

    let object = nodes.parent_node(property.id());
    let AstKind::ObjectExpression(_) = object.kind() else { return false };

    let call = nodes.parent_node(object.id());
    let AstKind::CallExpression(call_expr) = call.kind() else { return false };
    let object_span = object.kind().span();
    call_expr
        .arguments
        .iter()
        .any(|arg| arg.span() == object_span)
}

/// Check if a function/arrow body is exactly one `return <CallExpression>;`,
/// i.e. it forwards another call's result. Such a function delegates its
/// `Promise` return to the callee; `async` declares the `Promise` return type
/// (mirroring the companion `promise-function-async` rule) and dropping it would
/// break the type contract, so the absent `await` is not a smell. This is the
/// block-body analog of the already-exempt concise arrow `async () => call()`.
fn body_is_single_return_call(body: &oxc_ast::ast::FunctionBody) -> bool {
    let [oxc_ast::ast::Statement::ReturnStatement(ret)] = body.statements.as_slice() else {
        return false;
    };
    matches!(
        ret.argument,
        Some(oxc_ast::ast::Expression::CallExpression(_))
    )
}

/// Check if a method node or its class has decorators.
fn has_decorators(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(func_node.id()) {
        if let AstKind::MethodDefinition(method) = ancestor.kind() {
            if !method.decorators.is_empty() {
                return true;
            }
            // Check class decorators.
            for class_ancestor in semantic.nodes().ancestors(ancestor.id()) {
                if let AstKind::Class(class) = class_ancestor.kind() {
                    if !class.decorators.is_empty() {
                        return true;
                    }
                    break;
                }
            }
            return false;
        }
    }
    false
}

/// Check if the async function is a direct member of a class that declares an
/// `implements` clause. The immediate parent must be a `MethodDefinition`
/// (`async formData() {}`) or a `PropertyDefinition` (`handleError = async () =>
/// {}`); a nested arrow inside a method body is not a class member and is not
/// covered. comply is syntactic and cannot read the implemented interface, but
/// `async` on a member of an `implements`-ing class is the standard way to
/// satisfy a Promise-returning interface method without writing the explicit
/// return annotation, so the missing `await` is not a smell.
fn is_method_of_implementing_class(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let member = nodes.parent_node(func_node.id());
    if !matches!(
        member.kind(),
        AstKind::MethodDefinition(_) | AstKind::PropertyDefinition(_)
    ) {
        return false;
    }
    enclosing_class(member.id(), nodes).is_some_and(|class| ClassShape::of(class).has_implements)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return Vec::new();
        }

        // Collect node IDs of functions/arrows that contain an await or for-await.
        let mut has_await: FxHashSet<oxc_semantic::NodeId> =
            FxHashSet::default();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::AwaitExpression(_) => {
                    if let Some(func_id) = nearest_function_id(node, semantic) {
                        has_await.insert(func_id);
                    }
                }
                AstKind::ForOfStatement(for_of) if for_of.r#await => {
                    if let Some(func_id) = nearest_function_id(node, semantic) {
                        has_await.insert(func_id);
                    }
                }
                _ => {}
            }
        }

        // Now check all async functions/arrows.
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let (is_async, return_type, span, has_body) = match node.kind() {
                AstKind::Function(f) => (f.r#async, &f.return_type, f.span, f.body.is_some()),
                AstKind::ArrowFunctionExpression(f) => {
                    (f.r#async, &f.return_type, f.span, true)
                }
                _ => continue,
            };

            if !is_async || !has_body {
                continue;
            }

            if has_promise_return_type(ctx.source, return_type) {
                continue;
            }

            if has_decorators(node, semantic) {
                continue;
            }

            // Async method/property of a class with an `implements` clause. The
            // interface controls the contract (commonly `(): Promise<T>`), and
            // `async` is the standard way to satisfy it without an explicit
            // return annotation, so the missing `await` is not a smell. Members
            // of a class without `implements`, and standalone functions, stay
            // flagged.
            if is_method_of_implementing_class(node, semantic) {
                continue;
            }

            // Async callback passed to a call (framework route handler, event
            // listener, etc.). The callee controls the contract: it frequently
            // requires a `() => Promise<T>` signature, and `async` is also
            // load-bearing for sync-throw safety (a synchronous `throw` becomes
            // a rejected Promise the framework handles uniformly). The author
            // does not own the call site, so flagging the missing `await` here
            // is noise. Standalone/named async functions stay flagged.
            if is_call_argument(node, semantic) {
                continue;
            }

            // Async function used as a JSX attribute value (`<form action={async
            // () => {}}>`). Same rationale as a call argument: the attribute's
            // prop type owns the contract (Next.js App Router `action` is typed
            // `() => Promise<void>`), so `async` is required even when the body
            // fires a bound server action without awaiting it.
            if is_jsx_attribute_value(node, semantic) {
                continue;
            }

            // Async property of an object literal passed to a call, whether a
            // shorthand method (`$config({ async run() {} })`) or an arrow value
            // (`useForm({ onSubmit: async () => {} })`). The callback's `async`
            // signature is mandated by the callee's type (`onSubmit: (...) =>
            // Promise<T>`) even when the body declares resources synchronously or
            // never awaits.
            if is_object_property_in_call_arg(node, semantic) {
                continue;
            }

            if has_await.contains(&node.id()) {
                continue;
            }

            // better-result: `Result.gen(async function* () { yield* Result.await(...) })`
            // The wrapping async has no direct `await` but is justified by the Result pipeline.
            let body_text = match node.kind() {
                AstKind::Function(f) => f.body.as_ref().map(|b| {
                    &ctx.source[b.span.start as usize..b.span.end as usize]
                }),
                AstKind::ArrowFunctionExpression(f) => {
                    Some(&ctx.source[f.body.span().start as usize..f.body.span().end as usize])
                }
                _ => None,
            };
            if let Some(text) = body_text {
                if text.contains("Result.await") || text.contains("Result.gen") {
                    continue;
                }
            }

            // Arrow with concise-body returning a value (`async () => X`).
            // The companion `promise-function-async` rule mandates the
            // `async` keyword whenever the surrounding type contract
            // expects a Promise — even when the body returns a constant
            // (`async () => EMPTY` to satisfy `(): Promise<T[]>`).
            // Flagging missing-await here makes the two rules impossible
            // to satisfy together. Skip any concise-body arrow.
            if let AstKind::ArrowFunctionExpression(arrow) = node.kind()
                && arrow.expression
            {
                continue;
            }

            // Block body that is either empty (`async () => {}`) or exactly
            // `return <call>();`. An empty body is a `Promise<void>` no-op whose
            // `async` is its only source of the return type — dropping it yields
            // `() => void` and breaks a contextual `(params) => Promise<void>`. A
            // single-return-call forwards another call's `Promise`. In both cases
            // `async` is load-bearing for the type contract (per
            // `promise-function-async`), so the absent `await` is not a smell.
            // These are the block-body analogs of the concise arrow exemption.
            let block_body = match node.kind() {
                AstKind::Function(f) => f.body.as_deref(),
                AstKind::ArrowFunctionExpression(f) if !f.expression => Some(&*f.body),
                _ => None,
            };
            if block_body.is_some_and(|body| {
                body.statements.is_empty() || body_is_single_return_call(body)
            }) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "no-async-without-await".into(),
                message: "`async` function never awaits — drop the `async` keyword \
                          or add the `await` that justifies it."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_on_tsx(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn allows_result_await_pattern() {
        let src = r#"const run = async () => { return Result.gen(async function* () { const v = yield* Result.await(fetch()); return v; }); };"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_gen_pattern() {
        let src = r#"async function handler() { return Result.gen(async function* () { yield* Result.await(doStuff()); }); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_arrow_forwarding_promise_fn() {
        // Regression for rbaumier/comply#20 — `promise-function-async`
        // mandates async on Promise-returning arrows, then this rule
        // would trip on the missing await. Skip CallExpression bodies.
        assert!(run_on("const f = async () => fetch('/api');").is_empty());
        assert!(run_on("const g = async () => doStuff();").is_empty());
    }

    #[test]
    fn allows_async_arrow_returning_constant() {
        // Regression for rbaumier/comply#67 — concise-body arrow whose
        // expression is a non-call (constant / identifier / literal).
        // The async keyword can be load-bearing for the Promise return
        // type contract even when the body has no await.
        assert!(run_on("const f = async () => EMPTY;").is_empty());
        assert!(run_on("const f = async () => 42;").is_empty());
        assert!(run_on("const f = async () => [];").is_empty());
    }

    // Regression for #283: a no-op `Promise<void>` stub must be expressible as
    // an async function without tripping this rule — otherwise it contradicts
    // `promise-function-async` (which mandates the `async`). The delegated
    // `require-await`, which lacked these exceptions, was dropped in favour of
    // this rule.
    #[test]
    fn allows_empty_async_promise_void_stub() {
        assert!(run_on("async function noopAsync(): Promise<void> {}").is_empty());
    }

    #[test]
    fn allows_async_arrow_promise_void_stub() {
        assert!(run_on("const noopAsync = async (): Promise<void> => undefined;").is_empty());
    }

    #[test]
    fn allows_async_callback_passed_to_call() {
        // Regression for rbaumier/comply#1108 — async route handler registered
        // with a framework. The callee controls the contract and `async` is
        // intentional for sync-throw safety, so the missing await is not a smell.
        let src = r#"fastify.get("/v8/artifacts/status", async (_request, reply) => {
            return reply.send({ status: "enabled" });
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_callback_with_sync_throw() {
        // Second example from rbaumier/comply#1108 — a block-body async handler
        // whose only justification for `async` is sync-throw safety.
        let src = r#"fastify.post("/v8/artifacts/events", async (request, reply) => {
            if (!Array.isArray(request.body)) {
                throw new Error("Invalid request body.");
            }
            reply.code(200).send({});
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_run_method_in_sst_config() {
        // Regression for rbaumier/comply#1773 — SST's own project template.
        // `async run()` is a shorthand method in the object literal passed to
        // `$config(...)`; the framework types it `() => Promise<any>`, so async
        // is mandatory even though the body never awaits.
        let src = r#"export default $config({
            app(input) {
                return { name: "app", home: "aws" };
            },
            async run() {},
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_run_method_declaring_resources() {
        // Second example from rbaumier/comply#1773 — resources are declared via
        // synchronous constructor side effects, no await, but the framework
        // still requires the method to be async.
        let src = r#"export default $config({
            app(input) { return { name: "aws-workflow-python", home: "aws" }; },
            async run() {
                const workflow = new sst.aws.Workflow("Workflow", {});
                return { workflow: workflow.name };
            },
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_arrow_value_in_call_options_object() {
        // Regression for rbaumier/comply#1600 — TanStack Form `onSubmit` callback.
        // The arrow is an object-property *value* (not a shorthand method) in the
        // options object passed to `useForm(...)`; the library types the property
        // `(...) => Promise<T>`, so `async` is required even with no await.
        let src = r#"const form = useForm({
            onSubmit: async ({ value }) => {
                console.log(value)
            },
        });"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_block_body_forwarding_call() {
        // Regression for rbaumier/comply#1600 — `FieldGroupApi`-style delegation.
        // A block body that is exactly `return <call>();` forwards another call's
        // Promise; `async` declares the Promise return type (per
        // `promise-function-async`), so the absent await is not a smell.
        let src = r#"class FieldGroupApi {
            validateArrayFieldsStartingFrom = async (field, index, cause) => {
                return this.form.validateArrayFieldsStartingFrom(field, index, cause);
            };
        }"#;
        assert!(run_on(src).is_empty());
        // Same shape as a standalone async function.
        assert!(run_on("async function f() { return delegate(); }").is_empty());
    }

    #[test]
    fn still_flags_async_block_body_returning_non_call() {
        // Negative space for #1600: a block body whose return is not a call (here
        // a member access) has no forwarded Promise to justify `async` — it stays
        // flagged. Guards the forwarding exemption against over-broadening.
        assert_eq!(run_on("async function f() { return this.value; }").len(), 1);
    }

    #[test]
    fn still_flags_async_arrow_value_outside_call() {
        // Negative space for #1600: an async arrow property value in a plain object
        // (not a call argument) has no callee contract — it stays flagged.
        let src = "const handlers = { onSubmit: async () => { doSync(); } };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_arrow_as_jsx_attribute_value() {
        // Regression for rbaumier/comply#2211 — Next.js App Router progressive
        // enhancement. The async arrow is a JSX attribute value (`action={...}`);
        // the `action` prop type contract requires `() => Promise<void>`, so
        // `async` is mandatory even though the body only fires a bound server
        // action without awaiting it. The author does not own the call site.
        let src = r#"function DeleteItemButton() {
            return (
                <form
                    action={async () => {
                        optimisticUpdate(merchandiseId, "delete");
                        removeItemAction();
                    }}
                >
                    <button type="submit">Delete</button>
                </form>
            );
        }"#;
        assert!(run_on_tsx(src).is_empty());
    }

    #[test]
    fn still_flags_async_arrow_outside_jsx_attribute() {
        // Negative space for #2211: an ordinary async function with no await that
        // is not a call argument nor a JSX attribute value has no external
        // contract — it stays flagged even in a .tsx file.
        assert_eq!(run_on_tsx("async function f() { return 1; }").len(), 1);
    }

    #[test]
    fn allows_async_method_of_implementing_class() {
        // Regression for rbaumier/comply#1678 — a class method marked `async` to
        // satisfy a Promise-returning interface method (Web API `Body.formData(): \
        // Promise<FormData>`), with no `await` and no explicit return annotation.
        let src = r#"class ElysiaRequest implements Body {
            async formData() {
                if (this.init?.body instanceof FormData) return this.init.body;
                throw new Error('Unable to parse body as FormData');
            }
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_property_arrow_of_implementing_class() {
        // Second covered shape — an async class-property arrow (assigned to a
        // class field) in a class that declares `implements`.
        let src = r#"class Handler implements Contract {
            handle = async (x: number) => { return x; };
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_async_method_of_non_implementing_class() {
        // Negative space (a): an async method with no await in a class WITHOUT an
        // `implements` clause has no external contract to satisfy — stays flagged.
        let src = r#"class Plain {
            async formData() { return 42; }
        }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_nested_async_arrow_in_implementing_class_method() {
        // The exemption is gated on the function being a *direct* class member.
        // An async arrow nested inside a method body is not a class member, so it
        // stays flagged even though the enclosing class declares `implements`.
        let src = r#"class C implements I {
            async run() {
                const inner = async () => { return 1; };
                return inner;
            }
        }"#;
        // Only the nested arrow is flagged; the `run` method awaits nothing but is
        // exempt as a member of an implementing class.
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_object_method_not_in_call() {
        // An object method outside any call argument is an ordinary async
        // function without await — it stays flagged.
        let src = "const obj = { async run() { return 42; } };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_iife_without_await() {
        // An immediately-invoked async arrow is the callee, not an argument, so
        // it is not a framework callback and stays flagged.
        let src = "(async () => { return 42; })();";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_empty_block_body_async_arrow() {
        // Regression for rbaumier/comply#3850 — drizzle's `invalidate: async
        // (_params) => {}`. An empty-block-body async is a `Promise<void>` no-op
        // whose `async` is its only source of the return type; dropping it would
        // make it `() => void` and break a contextual `(params) => Promise<void>`.
        // Block-body analog of the concise-constant arrow and the annotated stub.
        assert!(run_on("const f = async (_params) => {};").is_empty());
    }

    #[test]
    fn allows_empty_block_body_async_function() {
        // Same shape as a standalone async function declaration with an empty body.
        assert!(run_on("async function noop() {}").is_empty());
    }

    #[test]
    fn still_flags_async_function_body_without_await() {
        let src = "async function f() { return 42; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_async_without_await() {
        let d = run_on("async function f() { return 42; }");
        assert_eq!(d.len(), 1);
    }
}

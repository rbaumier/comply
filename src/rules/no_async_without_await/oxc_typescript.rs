//! no-async-without-await OXC backend — flag `async` functions that contain
//! no `await` or `for await` in their own body.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
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

/// Check if the async function is a shorthand method of an object literal that
/// is passed as an argument to a call expression, e.g.
/// `$config({ async run() {} })`. The walk is `Function -> ObjectProperty ->
/// ObjectExpression -> CallExpression(arguments)`. Like an arrow callback, the
/// callee owns the contract: framework-config entry points such as SST/Pulumi's
/// `run()` are typed `() => Promise<T>`, so `async` is mandatory even when the
/// body declares resources via synchronous constructors and never awaits.
fn is_object_method_in_call_arg(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();

    let property = nodes.parent_node(func_node.id());
    let AstKind::ObjectProperty(prop) = property.kind() else { return false };
    if !prop.method {
        return false;
    }

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
        let mut has_await: std::collections::HashSet<oxc_semantic::NodeId> =
            std::collections::HashSet::new();

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

            // Async shorthand method of an object literal passed to a call,
            // e.g. SST/Pulumi `$config({ async run() {} })`. The framework-config
            // callback's `async` signature is mandated by the callee's type
            // (`run: () => Promise<T>`) even when resources are declared via
            // synchronous constructors and nothing is awaited.
            if is_object_method_in_call_arg(node, semantic) {
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

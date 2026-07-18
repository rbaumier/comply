//! structured-api-error oxc backend — flag `new Error()` inside a route-handler callback.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{CallExpression, Expression};
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];

/// Conventional identifier names for a web framework router. A route-method call
/// only signals a route file when chained on one of these receivers, so that
/// arbitrary `<obj>.get(`/`.delete(` calls (e.g. `map.get`, `set.delete`,
/// `this.blobContext.delete` on an Azure SDK REST client) are not misread as
/// route registrations.
const ROUTER_RECEIVERS: &[&str] = &[
    "app", "router", "server", "route", "api", "fastify", "koa", "hono", "srv", "r",
];

/// Whether a call is a route registration — a `<router>.<method>(...)` call on a
/// conventional receiver whose first argument is a path-like string literal
/// (starting with `/`). Excludes settings accessors such as `app.get('etag fn')`
/// whose argument is a settings key, not a route path.
fn call_is_route_registration(call: &CallExpression, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !ROUTE_METHODS.contains(&member.property.name.as_str()) {
        return false;
    }
    let Expression::Identifier(recv) = &member.object else {
        return false;
    };
    if !ROUTER_RECEIVERS.contains(&recv.name.as_str()) {
        return false;
    }
    call.arguments
        .first()
        .is_some_and(|arg| first_arg_is_route_path(arg, source))
}

/// Whether a call's first argument is a path-like string — a string or template
/// literal whose first character is `/`. Distinguishes a route registration
/// `app.get('/users', h)` from a settings accessor `app.get('etag fn')`.
fn first_arg_is_route_path(arg: &oxc_ast::ast::Argument, source: &str) -> bool {
    use oxc_ast::ast::Argument;
    match arg {
        Argument::StringLiteral(lit) => lit.value.as_str().starts_with('/'),
        Argument::TemplateLiteral(tpl) => tpl
            .quasis
            .first()
            .and_then(|q| source.get(q.span.start as usize..q.span.end as usize))
            .is_some_and(|raw| raw.starts_with('/')),
        _ => false,
    }
}

/// Whether `node` sits lexically inside a route-handler callback — a function or
/// arrow passed as an argument to a `<router>.<method>('/path', …)` registration.
/// Walks every enclosing function, so a `new Error` nested in blocks / `if` / `try`
/// inside the handler still resolves to it, and a handler in any argument position
/// (after middleware) is recognized. A handler passed as a named function
/// reference (`app.get('/x', handleX)`) is not reached by this lexical walk.
fn is_inside_route_handler(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        if !matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            continue;
        }
        if let AstKind::CallExpression(call) = nodes.parent_node(ancestor.id()).kind()
            && call_is_route_registration(call, source)
        {
            return true;
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        if ctx.file.path_segments.in_test_dir {
            return;
        }

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "Error" {
            return;
        }

        if !is_inside_route_handler(node, semantic, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bare `new Error()` in route handler \u{2014} use a structured error with `{ type, code, status, detail }`.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bare_error_in_route_file() {
        let src = r#"
import { Hono } from "hono";
app.get("/foo", (c) => {
    throw new Error("not found");
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bare_error_with_router_post() {
        let src = r#"
router.post("/y", (req, res) => {
    throw new Error("bad");
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bare_error_in_express_route_file() {
        let src = r#"
import express from "express";
const app = express();
app.post("/login", (req, res) => {
    throw new Error("bad");
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bare_error_in_fastify_route_file() {
        let src = r#"
import Fastify from 'fastify'
const fastify = Fastify()
fastify.get('/users', async (req, reply) => {
    throw new Error('x')
})
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_error_in_nestjs_decorator_controller() {
        // NestJS registers routes via decorators, not a `<router>.<method>('/path', …)`
        // call, so its handler methods are not reached by the callback-argument
        // ancestor walk. Known false-negative, tracked in #7902.
        let src = r#"
import { Controller, Get } from '@nestjs/common'
@Controller('users')
export class UsersController {
    @Get()
    find() {
        throw new Error('boom')
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_with_fastify_type_import_and_no_route() {
        // Issue #7755: `FastifyBaseLogger` is a type-only import threaded through the
        // DI logger; the file registers no route, so the precondition guard throw is
        // not an API response.
        let src = r#"
import { FastifyBaseLogger } from 'fastify'
function loadSecret(log: FastifyBaseLogger, secret?: string) {
    if (!secret) {
        throw new Error('Secret value is empty or binary')
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_with_express_type_only_import_and_no_route() {
        // Issue #7755: a `import type` from a method-call framework registers no
        // route, so the file must not be classified as a route module.
        let src = r#"
import type { Request } from 'express'
function parse(req: Request) {
    if (!req.body) {
        throw new Error('missing body')
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_in_non_route_file() {
        let src = r#"
function validate(x: string) {
    throw new Error("invalid input");
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_error_with_azure_sdk_delete_call() {
        // Azure SDK REST client: `.delete(` on a non-router receiver, no web import.
        let src = r#"
async function deleteBlob() {
    const response = await this.blobContext.delete({ abortSignal });
    if (!response.ok) {
        throw new Error("delete failed");
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_with_map_get() {
        let src = r#"
function lookup(map: Map<string, number>, k: string) {
    if (!map.get(k)) {
        throw new Error("missing");
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_with_set_delete() {
        let src = r#"
function drop(set: Set<string>, v: string) {
    if (!set.delete(v)) {
        throw new Error("not present");
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_with_identifier_ending_in_router_name() {
        // `clear.get(` must not match the single-letter `r` router receiver.
        let src = r#"
function read(clear: Map<string, number>, k: string) {
    if (!clear.get(k)) {
        throw new Error("missing");
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn flags_bare_error_with_single_letter_router() {
        let src = r#"
r.get("/x", (req, res) => {
    throw new Error("bad");
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_error_with_response_get() {
        let src = r#"
function header(response: Response) {
    const ct = response.get("content-type");
    if (!ct) {
        throw new Error("no content type");
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_with_searchparams_get() {
        // Issue #1788, example 1: `url.searchParams.get(` is not a route registration.
        let src = r#"
const jobParam = url.searchParams.get('job')
function pick(answer: string) {
    reject(new Error(`Invalid selection: ${answer}`))
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_with_map_get_and_headers_get() {
        // Issue #1788, example 2: `byModule.get(` + `request.headers.get(`.
        let src = r#"
const existing = byModule.get(adv.module_name) ?? []
function check(request: Request) {
    if (!isValidConnString(request.headers.get('x-connection-encrypted'))) {
        throw new Error("invalid")
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_with_headers_get_on_request_and_response() {
        // Issue #1788, example 3: `request.headers.get(` and `response.headers.get(`.
        let src = r#"
function client(request: Request, response: Response) {
    const retryAfterHeader = request.headers.get('Retry-After')
    const contentType = response.headers.get('Content-Type')
    if (!contentType) {
        throw new Error("no content type")
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_with_comment_and_settings_accessor() {
        // Issue #3390: Express `lib/application.js` — the only `app.get(`
        // occurrences are a comment and a settings lookup, not a route.
        let src = r#"
function set(setting, val) {
    if (arguments.length === 1) {
        // app.get(setting)
        return this.settings[setting];
    }
}
function engine(ext, fn) {
    var etagFn = app.get('etag fn');
    if (typeof fn !== 'function') {
        throw new Error('callback function required');
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn flags_bare_error_in_route_file_with_settings_accessor_present() {
        // A real route registration with a path arg still marks the file, even
        // when a settings accessor (`app.get('etag fn')`) is also present.
        let src = r#"
const etagFn = app.get('etag fn');
app.get('/users', (req, res) => {
    throw new Error('boom');
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_error_in_buildapp_factory_with_fastify_value_import() {
        // Issue #7832: `buildApp` is a server-construction factory (value-imports
        // `fastify`, only `.register(` calls, no route registration). Its boot-time
        // config check is not inside a route handler.
        let src = r#"
import fastify from "fastify";
function buildApp() {
    const server = fastify();
    server.register(somePlugin);
    if (!secretKey) {
        throw new Error("SECRET_KEY must be set in single-tenant mode.");
    }
    return server;
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_error_in_workspace_helper_with_fastify_request_type_import() {
        // Issue #7832: a `Result`-returning helper that type-imports `FastifyRequest`;
        // its bootstrap-state check is not inside a route handler.
        let src = r#"
import { FastifyRequest } from "fastify";
async function getWorkspace(req: FastifyRequest) {
    const workspace = await db().query.workspace.findFirst();
    if (!workspace) {
        throw new Error("No workspace found, ensure application is bootstrapped");
    }
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn allows_module_top_level_error_in_route_file() {
        // A `new Error` at module scope in a file that also registers a route is
        // not inside the handler callback.
        let src = r#"
app.get("/users", (req, res) => {
    res.send("ok");
});
const startupError = new Error("configuration missing");
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn flags_bare_error_nested_in_handler() {
        // A `new Error` nested in `if` inside the handler still resolves to the
        // handler callback.
        let src = r#"
app.post("/y", async (req, reply) => {
    if (bad) {
        throw new Error("boom");
    }
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_error_in_named_reference_handler() {
        // A handler passed as a named function reference is not lexically enclosed
        // by the route registration, so the ancestor walk does not reach it. Known
        // false-negative, tracked in #7902.
        let src = r#"
app.get("/x", handleX);
function handleX(req, res) {
    throw new Error("boom");
}
"#;
        assert!(run_on(src).is_empty(), "got: {:?}", run_on(src));
    }

    #[test]
    fn flags_bare_error_in_handler_after_middleware() {
        // The handler is a later argument (after middleware); it is still recognized.
        let src = r#"
app.get("/z", mw, (req, reply) => {
    throw new Error("boom");
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}

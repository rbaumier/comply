//! structured-api-error oxc backend — flag `new Error()` in route handler files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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

/// Import sources that mark a file as a web-framework route module even without
/// an `<router>.<method>(` call (e.g. NestJS decorator-based controllers).
const FRAMEWORK_IMPORTS: &[&str] =
    &["hono", "express", "fastify", "koa", "@nestjs", "elysia"];

/// Whether `line` imports from one of the known route frameworks, i.e. contains
/// `from 'pkg'`, `from "pkg"`, or (for scoped packages) the `@scope/` prefix.
fn imports_route_framework(line: &str) -> bool {
    FRAMEWORK_IMPORTS.iter().any(|pkg| {
        line.contains(&format!("from '{pkg}'"))
            || line.contains(&format!("from \"{pkg}\""))
            || (pkg.starts_with('@') && line.contains(&format!("{pkg}/")))
    })
}

/// Whether the file registers a route, i.e. contains a `<router>.<method>(...)`
/// call on a conventional receiver whose first argument is a path-like string
/// literal (starting with `/`). Walking the AST ignores comments and excludes
/// settings accessors such as `app.get('etag fn')` whose argument is a settings
/// key, not a route path.
fn has_route_registration(semantic: &oxc_semantic::Semantic, source: &str) -> bool {
    semantic.nodes().iter().any(|node| {
        let AstKind::CallExpression(call) = node.kind() else {
            return false;
        };
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
    })
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

fn is_route_file(semantic: &oxc_semantic::Semantic, source: &str) -> bool {
    has_route_registration(semantic, source)
        || source.lines().any(|line| imports_route_framework(line.trim()))
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

        if !is_route_file(semantic, ctx.source) {
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
router.post("/y", handler);
function handler() {
    throw new Error("bad");
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bare_error_in_express_import_file() {
        let src = r#"
import express from "express";
function handler() {
    throw new Error("bad");
}
"#;
        assert_eq!(run_on(src).len(), 1);
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
}

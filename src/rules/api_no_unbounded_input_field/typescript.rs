//! AST-only implementation.
//!
//! For each `call_expression` whose callee text is `z.string` / `z.number`
//! / `z.array`, walk the surrounding member-call chain to see if it ends in
//! `.max(...)`. If not, and the file is a server request-input boundary and is
//! not a test file and the enclosing top-level declaration is not a non-input
//! schema (response/output shapes like `*Response*Schema` / `*Select*Schema`,
//! or config/env shapes like `*Config*Schema` / `*Env*Schema` parsed from
//! `process.env`), and the field is not under a known output-contract key
//! (`response:`, `output:`, `returns:`, `result:`), emit a diagnostic.
//!
//! "Server request-input boundary" is gated by [`looks_like_api_path`], which
//! matches exact path components (`api`, `routes`, `controllers`, …) or
//! endpoint-handler filename stems (`route.ts`, `users.controller.ts`) — not a
//! mere `api`/`route` substring, so a feature folder like `apis/` or a file
//! like `delete-api.tsx` is not treated as an endpoint. Two file shapes are
//! skipped because their Zod schemas validate something other than a server
//! request body: a `"use client"` React component (in-browser form validation,
//! see [`is_client_component`]) and a TanStack Router page-route file using
//! `createFileRoute(...)` (parses the URL query, not a request body).

use crate::diagnostic::{Diagnostic, Severity};

/// Exact path-component names (directories, case-insensitive) that mark a
/// server endpoint location: Next.js `app/api/`, an Express/`src/routes/` tree,
/// NestJS `controllers/`, etc. Matched as whole path segments so `apis/` and a
/// feature folder merely containing `api` as a substring do not qualify.
const ENDPOINT_DIR_SEGMENTS: &[&str] = &[
    "api",
    "routes",
    "route",
    "handlers",
    "handler",
    "controllers",
    "controller",
    "endpoints",
    "endpoint",
];

/// Exact filename stems that mark a file as an endpoint handler: Next.js App
/// Router `route.ts`, and the bare `handler`/`controller`/`endpoint` stems.
const ENDPOINT_STEMS: &[&str] = &["route", "handler", "controller", "endpoint"];

/// Trailing stem segments that mark an endpoint handler file: NestJS
/// `users.controller.ts`, `auth.handler.ts`, `health.endpoint.ts`.
const ENDPOINT_STEM_SUFFIXES: &[&str] = &[".controller", ".handler", ".endpoint"];

/// True when `path` is a server HTTP request-input boundary: it has an exact
/// path component naming an endpoint directory ([`ENDPOINT_DIR_SEGMENTS`]), or
/// its filename stem marks an endpoint handler file (an exact endpoint stem, or
/// one ending in `.controller`/`.handler`/`.endpoint`). Segment/stem matching
/// (not substring) keeps `apis/`, `delete-api.tsx`, and feature folders that
/// merely contain `api`/`route` as a substring out of scope.
fn looks_like_api_path(path: &std::path::Path) -> bool {
    let has_endpoint_segment = path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|seg| ENDPOINT_DIR_SEGMENTS.iter().any(|d| seg.eq_ignore_ascii_case(d)))
    });
    if has_endpoint_segment {
        return true;
    }
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let stem = stem.to_ascii_lowercase();
    ENDPOINT_STEMS.iter().any(|s| stem == *s)
        || ENDPOINT_STEM_SUFFIXES.iter().any(|s| stem.ends_with(s))
}

/// True when `source` opens with a `"use client"` / `'use client'` directive
/// (with or without a trailing `;`) as one of its first non-empty lines. Such a
/// file is a client React component; its Zod schemas are in-browser form
/// validation, not a server request-input boundary.
fn is_client_component(source: &str) -> bool {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .any(|line| {
            let line = line.strip_suffix(';').unwrap_or(line).trim_end();
            line == "\"use client\"" || line == "'use client'"
        })
}

const TEST_MARKERS: &[&str] = &[
    ".test.",
    ".spec.",
    "__tests__",
    "_test.",
    ".e2e.",
    ".e2e-spec.",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        name.eq_ignore_ascii_case("tests") || name.eq_ignore_ascii_case("e2e")
    })
}

/// Substrings that mark a schema as something other than an HTTP request input:
/// response/output shapes (server-emitted) or config/env shapes (parsed from
/// `process.env` / static files, not from untrusted clients).
const NON_INPUT_NAME_MARKERS: &[&str] = &[
    "Response", "Output", "Return", "Detail", "Select", "Config", "EnvSchema",
];

/// Known keys that mark a schema as the server's output contract rather than a
/// request input. Fields under any of these keys must not be capped — server-emitted
/// response shapes in Elysia route descriptors or custom middleware.
const OUTPUT_CONTRACT_KEYS: &[&str] = &["response", "output", "returns", "result"];

/// True when `pair_node`'s parent object is itself an argument to a Zod schema
/// call (`z.*`). Distinguishes route-descriptor keys from same-named schema fields
/// (e.g. `body: z.object({ result: z.string() })`).
fn pair_inside_schema_call(pair_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(obj) = pair_node.parent() else { return false };
    if obj.kind() != "object" {
        return false;
    }
    let Some(args) = obj.parent() else { return false };
    if args.kind() != "arguments" {
        return false;
    }
    let Some(call) = args.parent() else { return false };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(func) = call.child_by_field_name("function") else { return false };
    let Ok(func_text) = func.utf8_text(source) else { return false };
    func_text.starts_with("z.")
}

/// True when `node` sits inside the value of a known output-contract property
/// (`response:`, `output:`, `returns:`, `result:`) at route-descriptor level.
/// Server-emitted shapes in Elysia route descriptors or custom middleware.
/// Capping truncates legitimate large payloads (e.g. CSV exports).
fn enclosed_in_response_field(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "pair"
            && parent.child_by_field_name("value").map(|v| v.id()) == Some(cur.id())
            && let Some(key) = parent.child_by_field_name("key")
            && let Ok(key_text) = key.utf8_text(source)
            && OUTPUT_CONTRACT_KEYS
                .iter()
                .any(|&k| key_text.trim_matches(|c| matches!(c, '"' | '\'' | '`')) == k)
            && !pair_inside_schema_call(parent, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// Walk up from `node`, looking for a `variable_declarator` ancestor whose
/// `name` field contains one of `NON_INPUT_NAME_MARKERS`. Returns true if
/// found. Used to skip top-level response/config-schema declarations.
fn enclosed_in_non_input_schema(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "variable_declarator"
            && let Some(name_node) = parent.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(source)
            && NON_INPUT_NAME_MARKERS.iter().any(|m| name.contains(m))
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// Walk up the member-call chain rooted at `call_node` and return true if
/// any `member_expression.property` along the way is `max`.
///
/// Chain shape: `z.string().max(100)` — the AST looks like
/// `call_expression(member_expression(call_expression(...), property: "max"))`.
fn chain_has_max(call_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = call_node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "member_expression" => {
                // We're the `object` of a member_expression. Check its property.
                let Some(obj) = parent.child_by_field_name("object") else {
                    break;
                };
                if obj.id() != cur.id() {
                    break;
                }
                if let Some(prop) = parent.child_by_field_name("property")
                    && let Ok(prop_text) = prop.utf8_text(source)
                    && prop_text == "max"
                {
                    return true;
                }
                cur = parent;
            }
            "call_expression" => {
                // We're being called: `.foo(...)` → continue walking.
                if let Some(func) = parent.child_by_field_name("function")
                    && func.id() == cur.id()
                {
                    cur = parent;
                    continue;
                }
                break;
            }
            _ => break,
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] prefilter = ["z.string", "z.number", "z.array"] =>
    |node, source, ctx, diagnostics|

    if !looks_like_api_path(ctx.path) {
        return;
    }
    if is_test_file(ctx.path) {
        return;
    }
    // A `"use client"` file validates a browser form, not a server request body.
    if is_client_component(ctx.source) {
        return;
    }
    // A TanStack Router page route parses the URL query, not a request body.
    if ctx.source.contains("createFileRoute") {
        return;
    }

    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    let kind = match name {
        "z.string" => "z.string",
        "z.number" => "z.number",
        "z.array" => "z.array",
        _ => return,
    };

    if chain_has_max(node, source) {
        return;
    }
    if enclosed_in_non_input_schema(node, source) {
        return;
    }
    if enclosed_in_response_field(node, source) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{kind}` has no `.max(N)` — unbounded API input is a resource-exhaustion vector."
        ),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, path)
    }

    #[test]
    fn flags_unbounded_string() {
        let src = "const Body = z.object({ name: z.string() });";
        assert_eq!(run_at(src, "src/routes/x.ts").len(), 1);
    }

    #[test]
    fn flags_unbounded_array() {
        let src = "const Body = z.object({ tags: z.array(z.string().max(20)) });";
        assert_eq!(run_at(src, "src/api/y.ts").len(), 1);
    }

    #[test]
    fn allows_string_with_max() {
        let src = "const Body = z.object({ name: z.string().max(100) });";
        assert!(run_at(src, "src/routes/x.ts").is_empty());
    }

    #[test]
    fn allows_chain_min_then_max() {
        let src = "const Body = z.object({ n: z.number().min(0).max(99) });";
        assert!(run_at(src, "src/routes/x.ts").is_empty());
    }

    #[test]
    fn ignores_non_api_files() {
        let src = "const X = z.object({ name: z.string() });";
        assert!(run_at(src, "src/lib/util.ts").is_empty());
    }

    #[test]
    fn ignores_response_schema_by_name() {
        // Regression for #80 — response/select schemas are server-emitted,
        // not user inputs, and don't need `.max()`.
        let src = "export const OrganizationDetailSchema = z.object({\n  teams: z.array(TeamSelectSchema),\n  members: z.array(OrganizationMemberSchema),\n});";
        assert!(run_at(src, "src/api/orgs.ts").is_empty());
    }

    #[test]
    fn still_flags_input_schema_alongside_response() {
        let src = "export const CreateOrgInputSchema = z.object({ name: z.string() });\nexport const OrgResponseSchema = z.object({ teams: z.array(Team) });";
        let diags = run_at(src, "src/api/orgs.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("z.string"));
    }

    #[test]
    fn ignores_z_string_inside_string_literal() {
        // Regression for #106 — `z.string()` appearing as text content of a
        // template/string literal (test fixture data) must not be flagged.
        let src = r#"test.each([
  [`.body(z.object({ id: z.string() }))`, true],
])('flags inline wire schema: %s', (line, expected) => {
  expect(lineHasInlineWireSchema(line)).toBe(expected);
});"#;
        // Even outside a test file the AST should treat the template literal
        // as a single string node; no `call_expression` for `z.string()` exists
        // inside it.
        assert!(run_at(src, "src/api/inline.ts").is_empty());
    }

    #[test]
    fn ignores_env_config_schema() {
        // Regression for #187 — schemas whose name marks them as config/env
        // are parsed against `process.env`, not HTTP request bodies. They
        // are not a resource-exhaustion vector.
        let src = "export const observabilityConfigSchema = z.object({\n  otelServiceName: z.string().trim().min(1).default(\"amadeo\"),\n});";
        assert!(run_at(src, "src/api/observability/config.ts").is_empty());
    }

    #[test]
    fn ignores_env_schema_by_name() {
        let src = "export const AppEnvSchema = z.object({ apiUrl: z.string() });";
        assert!(run_at(src, "src/api/env.ts").is_empty());
    }

    #[test]
    fn flags_envelope_schema_in_api_path() {
        // Regression: `WebhookEnvelopeSchema` contains "Env" but must not be
        // exempted — only "EnvSchema" is the non-input marker.
        let src = "export const WebhookEnvelopeSchema = z.object({ id: z.string() });";
        assert_eq!(run_at(src, "src/api/webhooks.ts").len(), 1);
    }

    #[test]
    fn ignores_response_field_on_route_descriptor() {
        // Regression for #285 — `response:` is the server's output contract, not
        // a request input. Capping it would truncate legitimate large payloads.
        let src = "app.get('/extract', handler, {\n  query: ExtractQuerySchema,\n  response: z.string(),\n  detail: { tags: ['x'] },\n});";
        assert!(run_at(src, "src/api/features/laboratories/extract.ts").is_empty());
    }

    #[test]
    fn still_flags_query_field_on_route_descriptor() {
        let src = "app.get('/extract', handler, {\n  query: z.object({ q: z.string() }),\n  response: z.string(),\n});";
        let diags = run_at(src, "src/api/features/laboratories/extract.ts");
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn ignores_test_files() {
        // Regression for #106 — `z.string()` inside *.test.ts is fixture data.
        let src = "const Body = z.object({ name: z.string() });";
        assert!(run_at(src, "src/api/features/no-inline-wire-schemas.test.ts").is_empty());
        assert!(run_at(src, "src/api/foo.spec.ts").is_empty());
        assert!(run_at(src, "src/api/__tests__/foo.ts").is_empty());
    }

    #[test]
    fn ignores_response_field_with_async_handler() {
        // Regression for #383 — amadeo CSV routes use async handlers.
        let src = "new Elysia().get(\n  '/extract',\n  async ({ query, set }) => handler(),\n  {\n    query: FiltersSchema,\n    response: z.string(),\n    detail: { tags: ['x'] },\n  },\n);";
        assert!(run_at(src, "src/api/features/products/extract-products-csv.ts").is_empty());
    }

    #[test]
    fn ignores_output_field_on_route_descriptor() {
        // Regression for #383 — `output:` is another naming convention for the
        // server's response contract used in some Elysia middleware variants.
        let src = "app.get('/items', handler, {\n  body: ItemBodySchema,\n  output: z.string(),\n});";
        assert!(run_at(src, "src/api/features/items.ts").is_empty());
    }

    #[test]
    fn ignores_returns_field_on_route_descriptor() {
        // Regression for #383 — `returns:` mirrors `response:` semantics.
        let src = "app.post('/send', handler, {\n  body: SendBodySchema,\n  returns: z.string(),\n});";
        assert!(run_at(src, "src/api/features/send.ts").is_empty());
    }

    #[test]
    fn ignores_result_field_on_route_descriptor() {
        // Regression for #383 — `result:` is used in some custom middleware stacks.
        let src = "app.get('/fetch', handler, {\n  query: FetchQuerySchema,\n  result: z.string(),\n});";
        assert!(run_at(src, "src/api/features/fetch.ts").is_empty());
    }

    #[test]
    fn still_flags_body_field_on_route_descriptor() {
        // Ensure that only known output-contract keys are exempted; request body
        // fields must still be flagged.
        let src = "app.post('/create', handler, {\n  body: z.object({ name: z.string() }),\n  response: z.string(),\n});";
        let diags = run_at(src, "src/api/features/create.ts");
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn still_flags_body_field_named_result() {
        // Regression for #383 — a request body field named `result` (or `output`,
        // `returns`) must still be flagged. Only top-level route-descriptor
        // output-contract keys are exempt, not schema fields with coincident names.
        let src = "app.post('/create', handler, {\n  body: z.object({ result: z.string() }),\n  response: z.string(),\n});";
        let diags = run_at(src, "src/api/features/create.ts");
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn ignores_tanstack_page_route() {
        // Regression for #3709 — a TanStack Router page route's `validateSearch`
        // schema parses the URL query, not a request body. The `routes/` segment
        // still gates the path, so `createFileRoute` is the distinguishing signal.
        let src = "import { createFileRoute } from \"@tanstack/react-router\";\nconst searchSchema = z.object({ session: z.string().optional() });\nexport const Route = createFileRoute(\"/\")({ validateSearch: searchSchema });";
        assert!(run_at(src, "apps/portal/src/routes/index.tsx").is_empty());
    }

    #[test]
    fn ignores_use_client_in_apis_path() {
        // Regression for #3709 — a `"use client"` form schema under an `apis/`
        // feature folder is in-browser validation. Both the use-client directive
        // and the (now exact-segment) path gate keep it out of scope.
        let src = "\"use client\";\nconst formSchema = z.object({ name: z.string() });";
        assert!(
            run_at(
                src,
                "apps/dashboard/app/(app)/[workspaceSlug]/apis/[apiId]/settings/components/delete-api.tsx"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_apis_resource_dir_without_use_client() {
        // Regression for #3709 — path tightening alone: an `apis/` resource dir is
        // not an exact `api` segment and the stem `x` is not an endpoint handler,
        // so a plain schema there is not a server input boundary.
        let src = "const formSchema = z.object({ name: z.string() });";
        assert!(run_at(src, "apps/dashboard/src/apis/x.tsx").is_empty());
    }

    #[test]
    fn ignores_use_client_in_api_path() {
        // Regression for #3709 — a `"use client"` file inside a genuine `api/`
        // segment is still a client component, so its schemas are skipped.
        let src = "\"use client\";\nconst s = z.object({ x: z.string() });";
        assert!(run_at(src, "src/api/foo.tsx").is_empty());
    }

    #[test]
    fn flags_nextjs_route_handler() {
        // The Next.js App Router `route.ts` stem plus the `api` segment marks a
        // real server endpoint; an unbounded request-body field is still flagged.
        let src = "const Body = z.object({ name: z.string() });";
        assert_eq!(run_at(src, "app/api/users/route.ts").len(), 1);
    }
}

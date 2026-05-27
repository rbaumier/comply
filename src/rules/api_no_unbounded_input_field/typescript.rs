//! AST-only implementation.
//!
//! For each `call_expression` whose callee text is `z.string` / `z.number`
//! / `z.array`, walk the surrounding member-call chain to see if it ends in
//! `.max(...)`. If not, and the file lives in a route/api path and is not a
//! test file and the enclosing top-level declaration is not a non-input
//! schema (response/output shapes like `*Response*Schema` / `*Select*Schema`,
//! or config/env shapes like `*Config*Schema` / `*Env*Schema` parsed from
//! `process.env`), emit a diagnostic.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_HINTS: &[&str] = &["route", "api", "handler", "controller", "endpoint"];

fn looks_like_api_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    ROUTE_HINTS.iter().any(|h| s.contains(h))
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

/// True when `node` sits inside the value of a `response:` property — an
/// Elysia/route-descriptor output contract, not a request input. Capping the
/// server's own output would truncate legitimate large payloads (e.g. CSV).
fn enclosed_in_response_field(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "pair"
            && parent.child_by_field_name("value").map(|v| v.id()) == Some(cur.id())
            && let Some(key) = parent.child_by_field_name("key")
            && let Ok(key_text) = key.utf8_text(source)
            && key_text.trim_matches(|c| matches!(c, '"' | '\'' | '`')) == "response"
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
mod tests {
    use super::*;

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(s, &Check, path)
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
}

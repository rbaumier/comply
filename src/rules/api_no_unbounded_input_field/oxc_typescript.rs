use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use std::sync::Arc;

pub struct Check;

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

const NON_INPUT_NAME_MARKERS: &[&str] = &[
    "Response", "Output", "Return", "Detail", "Select", "Config", "EnvSchema",
];

const OUTPUT_CONTRACT_KEYS: &[&str] = &["response", "output", "returns", "result"];

fn z_call_kind<'a>(call: &oxc_ast::ast::CallExpression<'a>) -> Option<&'static str> {
    if let Expression::StaticMemberExpression(mem) = &call.callee {
        if let Expression::Identifier(obj) = &mem.object {
            if obj.name == "z" {
                return match mem.property.name.as_str() {
                    "string" => Some("z.string"),
                    "number" => Some("z.number"),
                    "array" => Some("z.array"),
                    _ => None,
                };
            }
        }
    }
    None
}

fn chain_has_max(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::StaticMemberExpression(mem) => {
                if mem.property.name == "max" {
                    return true;
                }
            }
            // Stop at statement or declaration boundaries
            AstKind::ExpressionStatement(_)
            | AstKind::VariableDeclarator(_)
            | AstKind::ReturnStatement(_) => break,
            _ => {}
        }
    }
    false
}

fn enclosed_in_non_input_schema(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::VariableDeclarator(decl) = ancestor.kind() {
            if let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &decl.id {
                let name = ident.name.as_str();
                if NON_INPUT_NAME_MARKERS.iter().any(|m| name.contains(m)) {
                    return true;
                }
            }
        }
    }
    false
}

fn prop_inside_schema_call(
    prop_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut saw_object_expr = false;
    for ancestor in semantic.nodes().ancestors(prop_node.id()) {
        match ancestor.kind() {
            AstKind::ObjectExpression(_) => {
                saw_object_expr = true;
            }
            AstKind::CallExpression(call) if saw_object_expr => {
                if let Expression::StaticMemberExpression(mem) = &call.callee {
                    if let Expression::Identifier(obj) = &mem.object {
                        return obj.name == "z";
                    }
                }
                return false;
            }
            _ => {}
        }
    }
    false
}

fn enclosed_in_response_field(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::ObjectProperty(prop) = ancestor.kind() {
            let key_name = match &prop.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            let key_trimmed = key_name.trim_matches(|c| matches!(c, '"' | '\'' | '`'));
            if OUTPUT_CONTRACT_KEYS.contains(&key_trimmed)
                && !prop_inside_schema_call(ancestor, semantic)
            {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.string", "z.number", "z.array"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !looks_like_api_path(ctx.path) {
            return;
        }
        if is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Some(kind) = z_call_kind(call) else {
            return;
        };
        if chain_has_max(node, semantic) {
            return;
        }
        if enclosed_in_non_input_schema(node, semantic) {
            return;
        }
        if enclosed_in_response_field(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{kind}` has no `.max(N)` — unbounded API input is a resource-exhaustion vector."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(s, &Check, path)
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
        let src = r#"test.each([
  [`.body(z.object({ id: z.string() }))`, true],
])('flags inline wire schema: %s', (line, expected) => {
  expect(lineHasInlineWireSchema(line)).toBe(expected);
});"#;
        assert!(run_at(src, "src/api/inline.ts").is_empty());
    }

    #[test]
    fn ignores_env_config_schema() {
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
        let src = "export const WebhookEnvelopeSchema = z.object({ id: z.string() });";
        assert_eq!(run_at(src, "src/api/webhooks.ts").len(), 1);
    }

    #[test]
    fn ignores_response_field_on_route_descriptor() {
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
        let src = "const Body = z.object({ name: z.string() });";
        assert!(run_at(src, "src/api/features/no-inline-wire-schemas.test.ts").is_empty());
        assert!(run_at(src, "src/api/foo.spec.ts").is_empty());
        assert!(run_at(src, "src/api/__tests__/foo.ts").is_empty());
    }

    #[test]
    fn ignores_response_field_with_async_handler() {
        let src = "new Elysia().get(\n  '/extract',\n  async ({ query, set }) => handler(),\n  {\n    query: FiltersSchema,\n    response: z.string(),\n    detail: { tags: ['x'] },\n  },\n);";
        assert!(run_at(src, "src/api/features/products/extract-products-csv.ts").is_empty());
    }

    #[test]
    fn ignores_output_field_on_route_descriptor() {
        let src = "app.get('/items', handler, {\n  body: ItemBodySchema,\n  output: z.string(),\n});";
        assert!(run_at(src, "src/api/features/items.ts").is_empty());
    }

    #[test]
    fn ignores_returns_field_on_route_descriptor() {
        let src = "app.post('/send', handler, {\n  body: SendBodySchema,\n  returns: z.string(),\n});";
        assert!(run_at(src, "src/api/features/send.ts").is_empty());
    }

    #[test]
    fn ignores_result_field_on_route_descriptor() {
        let src = "app.get('/fetch', handler, {\n  query: FetchQuerySchema,\n  result: z.string(),\n});";
        assert!(run_at(src, "src/api/features/fetch.ts").is_empty());
    }

    #[test]
    fn still_flags_body_field_on_route_descriptor() {
        let src = "app.post('/create', handler, {\n  body: z.object({ name: z.string() }),\n  response: z.string(),\n});";
        let diags = run_at(src, "src/api/features/create.ts");
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn still_flags_body_field_named_result() {
        let src = "app.post('/create', handler, {\n  body: z.object({ result: z.string() }),\n  response: z.string(),\n});";
        let diags = run_at(src, "src/api/features/create.ts");
        assert_eq!(diags.len(), 1, "{diags:?}");
    }
}

//! For every `call_expression` whose callee is `<x>.parse` or
//! `<x>.safeParse`, walk up the AST to find the enclosing function. If
//! that function does not look like an HTTP boundary, emit a
//! diagnostic.
//!
//! "Looks like an HTTP boundary" heuristic:
//!   - name contains `handler`, `middleware`, `controller`, `endpoint`,
//!     `resolver`
//!   - the function is the callback argument of a route registration
//!     call (`app.get(...)`, `router.post(...)`, …)
//!   - the function is `export async function GET/POST/PUT/PATCH/DELETE`
//!     (Next.js / Remix-style route exports — exact-uppercase verb name)
//!   - function has a parameter typed `Request` / `NextApiRequest` /
//!     similar, OR a parameter literally named `req`, `request`, `ctx`,
//!     `context`
//!
//! Names like `getUser` or `postProcess` are NOT treated as handlers
//! anymore — the verb prefix alone is too noisy.

use crate::diagnostic::{Diagnostic, Severity};

/// Lowercase built-in receivers whose `.parse(...)` is not a schema
/// validator: `path.parse` (split a filesystem path, from `node:path`).
/// Capitalized builtins (`JSON`, `URL`) are covered by the PascalCase
/// utility-class rule in `is_non_schema_parse_receiver`.
const BUILTIN_PARSE_RECEIVERS: &[&str] = &["path"];

/// True when the receiver `name` is not a schema validator: either a
/// lowercase builtin (`path`) or a PascalCase static utility class. Zod
/// schemas live in camelCase variables (`userSchema`) or PascalCase names
/// ending in `Schema` (`ConfigSchema`); a PascalCase receiver that does not
/// end in `Schema` (`SelectorParser`, `ValueParser`, `JSON`, `URL`) is a
/// static utility class, not a schema.
fn is_non_schema_parse_receiver(name: &str) -> bool {
    if BUILTIN_PARSE_RECEIVERS.contains(&name) {
        return true;
    }
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && !name.ends_with("Schema")
}

fn is_parse_call(callee: tree_sitter::Node, source: &[u8]) -> Option<&'static str> {
    if callee.kind() != "member_expression" {
        return None;
    }
    if let Some(object) = callee.child_by_field_name("object")
        && object.kind() == "identifier"
        && let Ok(recv) = std::str::from_utf8(&source[object.byte_range()])
        && is_non_schema_parse_receiver(recv)
    {
        return None;
    }
    let prop = callee.child_by_field_name("property")?;
    let name = std::str::from_utf8(&source[prop.byte_range()]).ok()?;
    match name {
        "parse" => Some("parse"),
        "safeParse" => Some("safeParse"),
        _ => None,
    }
}

const HANDLER_KEYWORDS: &[&str] = &[
    "handler",
    "middleware",
    "controller",
    "endpoint",
    "resolver",
];

const HTTP_VERB_EXPORTS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

fn name_looks_like_handler(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if HANDLER_KEYWORDS.iter().any(|k| lower.contains(k)) {
        return true;
    }
    // Next.js / Remix-style: an exact-uppercase HTTP verb name signals a
    // route-handler export. Lowercase / mixed forms (`getUser`,
    // `postProcess`) are NOT handlers.
    if HTTP_VERB_EXPORTS.contains(&name) {
        return true;
    }
    false
}

const REQUEST_PARAM_NAMES: &[&str] = &["req", "request", "ctx", "context"];

fn params_look_like_handler(params: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if child.kind() != "required_parameter" && child.kind() != "optional_parameter" {
            continue;
        }
        if let Some(pat) = child.child_by_field_name("pattern")
            && pat.kind() == "identifier"
            && let Ok(name) = std::str::from_utf8(&source[pat.byte_range()])
            && REQUEST_PARAM_NAMES.contains(&name)
        {
            return true;
        }
        if let Some(type_ann) = child.child_by_field_name("type")
            && let Ok(text) = std::str::from_utf8(&source[type_ann.byte_range()])
            && (text.contains("Request") || text.contains("NextApiRequest"))
        {
            return true;
        }
    }
    false
}

fn enclosing_function_info<'a>(
    mut node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<(Option<String>, tree_sitter::Node<'a>)> {
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "function_declaration" | "method_definition" => {
                let name = parent
                    .child_by_field_name("name")
                    .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
                    .map(|s| s.to_string());
                return Some((name, parent));
            }
            "arrow_function" | "function_expression" => {
                // Try to resolve an assigned name via `variable_declarator`.
                let mut name = None;
                if let Some(gp) = parent.parent()
                    && gp.kind() == "variable_declarator"
                    && let Some(id) = gp.child_by_field_name("name")
                {
                    name = std::str::from_utf8(&source[id.byte_range()])
                        .ok()
                        .map(|s| s.to_string());
                }
                return Some((name, parent));
            }
            _ => {}
        }
        node = parent;
    }
    None
}

fn is_in_handler_context(fn_node: tree_sitter::Node, name: Option<&str>, source: &[u8]) -> bool {
    if let Some(n) = name
        && name_looks_like_handler(n)
    {
        return true;
    }
    if let Some(params) = fn_node.child_by_field_name("parameters")
        && params_look_like_handler(params, source)
    {
        return true;
    }
    if is_inline_route_callback(fn_node, source) {
        return true;
    }
    false
}

const ROUTE_VERBS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "head", "options", "all", "use",
];

/// True when `fn_node` (an arrow / function expression) is an argument
/// to a call like `<obj>.<verb>(...)`, where `<verb>` is a router HTTP
/// method name (`app.get`, `router.post`, …).
fn is_inline_route_callback(fn_node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(parent) = fn_node.parent() else {
        return false;
    };
    if parent.kind() != "arguments" {
        return false;
    }
    let Some(call) = parent.parent() else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    let Ok(method) = std::str::from_utf8(&source[prop.byte_range()]) else {
        return false;
    };
    ROUTE_VERBS.contains(&method)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    if node.kind() != "call_expression" { return; }
    let Some(callee) = node.child_by_field_name("function") else { return };
    let Some(method) = is_parse_call(callee, source) else { return };

    let Some((name, fn_node)) = enclosing_function_info(node, source) else {
        // Top-level parse call — treat as boundary (module init). Skip.
        return;
    };

    if is_in_handler_context(fn_node, name.as_deref(), source) {
        return;
    }

    let fn_label = name.as_deref().unwrap_or("<anonymous>");
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`.{method}(...)` called inside `{fn_label}` — validate at the HTTP boundary only; internal callers should trust the typed contract."
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_parse_in_internal_function() {
        let d = run("function computeTotal(input: unknown) { return Schema.parse(input); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("computeTotal"));
    }

    #[test]
    fn flags_safeparse_in_internal_arrow() {
        let d = run("const run = (x: unknown) => Schema.safeParse(x);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_parse_in_handler_named_function() {
        assert!(
            run("function userHandler(req: Request, res: Response) { Schema.parse(req.body); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_parse_in_function_with_request_param() {
        assert!(
            run("function run(req: Request, res: Response) { Schema.parse(req.body); }").is_empty()
        );
    }

    #[test]
    fn allows_parse_at_module_level() {
        assert!(run("const config = ConfigSchema.parse(process.env);").is_empty());
    }

    #[test]
    fn allows_parse_in_verb_prefixed_function_with_request_param() {
        // Still allowed because of the `req: Request` parameter, not
        // because of the `getUser` name.
        assert!(
            run("function getUser(req: Request) { return Schema.parse(req.body); }").is_empty()
        );
    }

    #[test]
    fn flags_parse_in_verb_prefixed_function_without_request_param() {
        // REVIEW regression: a function named `getUser`/`postProcess`
        // that takes no Request parameter is NOT a handler; the verb
        // prefix alone must not exempt it from the rule.
        let d = run("function getUser(input: unknown) { return Schema.parse(input); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getUser"));
    }

    #[test]
    fn allows_parse_in_inline_route_callback() {
        // Inline arrow handler passed to `.get(...)` is treated as a
        // boundary even when its name (or lack thereof) gives no signal.
        assert!(run("app.get('/u', (req, res) => { Schema.parse(req.body); });").is_empty());
    }

    #[test]
    fn allows_json_parse_in_internal_function() {
        // Issue #1738 example 1: `JSON.parse` is a built-in deserializer,
        // not a schema validator.
        let src = "async function formatFiles(cwd: string): Promise<void> { \
            const prettierPackageJson = JSON.parse(await readFile(prettierPath, 'utf-8')) as { bin: string }; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_path_parse_in_internal_function() {
        // Issue #1738 example 2: `path.parse` from `node:path` splits a
        // filesystem path; it is not a schema validator.
        let src = "function readOptions(cwd: string, yarn: boolean): PackageManagerOptions { \
            const root = path.parse(cwd).root; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_url_parse_in_internal_function() {
        assert!(run("function f(s: string) { return URL.parse(s); }").is_empty());
    }

    #[test]
    fn allows_parse_in_nextjs_uppercase_route_export() {
        assert!(
            run(
                "export async function GET(req: Request) { return Schema.parse(await req.json()); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_pascal_case_parser_class_parse() {
        // Issue #2153: PascalCase static parser utility classes
        // (`SelectorParser.parse`, `ValueParser.parse`) are not Zod schemas.
        assert!(
            run("function f(name: string) { let ast = SelectorParser.parse(name); return ast; }")
                .is_empty()
        );
        assert!(run("function h(x: unknown) { return MyParser.parse(x); }").is_empty());
    }

    #[test]
    fn flags_pascal_case_schema_suffix_parse() {
        // Negative space: a PascalCase receiver ending in `Schema` is a Zod
        // schema and must still flag inside a non-boundary helper.
        let d = run("function compute(input: unknown) { return ConfigSchema.parse(input); }");
        assert_eq!(d.len(), 1);
    }
}

//! api-validate-at-boundaries OXC backend.
//!
//! Flags `.parse(...)` / `.safeParse(...)` calls in functions that don't
//! look like request handlers or middleware.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const HANDLER_KEYWORDS: &[&str] = &[
    "handler",
    "middleware",
    "controller",
    "endpoint",
    "resolver",
];

const HTTP_VERB_EXPORTS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

const REQUEST_PARAM_NAMES: &[&str] = &["req", "request", "ctx", "context"];

const ROUTE_VERBS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "head", "options", "all", "use",
];

/// Built-in / standard-library receivers whose `.parse(...)` is not a
/// schema validator: `JSON.parse` (deserialize JSON), `path.parse`
/// (split a filesystem path, from `node:path`), `URL.parse` (parse a
/// URL string). These are never API-boundary validation calls.
const BUILTIN_PARSE_RECEIVERS: &[&str] = &["JSON", "path", "URL"];

/// True when the `.parse(...)` receiver is a built-in non-schema object
/// (e.g. `JSON.parse(...)`, `path.parse(...)`, `URL.parse(...)`).
fn is_builtin_parse_receiver(object: &Expression) -> bool {
    let Expression::Identifier(ident) = object else {
        return false;
    };
    BUILTIN_PARSE_RECEIVERS.contains(&ident.name.as_str())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // The "validate once at the boundary, trust internally" principle only
        // holds for HTTP API servers. In CLI tools and libraries, `.parse(...)`
        // validates external input (config files, runtime args) at the point of
        // reading — that IS the boundary — so the rule must stay silent.
        if !ctx.project.is_http_api_server() {
            return;
        }

        // Callee must be `.parse` or `.safeParse`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if method != "parse" && method != "safeParse" {
            return;
        }

        // Built-in `.parse(...)` (JSON/path/URL) is not schema validation.
        if is_builtin_parse_receiver(&member.object) {
            return;
        }

        // Find enclosing function
        let Some((fn_name, fn_node)) = enclosing_function_info(node, semantic, ctx.source)
        else {
            // Top-level parse call — treat as boundary (module init). Skip.
            return;
        };

        if is_in_handler_context(fn_node, fn_name.as_deref(), semantic, ctx.source) {
            return;
        }

        let fn_label = fn_name.as_deref().unwrap_or("<anonymous>");
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{method}(...)` called inside `{fn_label}` — validate at the HTTP boundary only; internal callers should trust the typed contract."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn name_looks_like_handler(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if HANDLER_KEYWORDS.iter().any(|k| lower.contains(k)) {
        return true;
    }
    if HTTP_VERB_EXPORTS.contains(&name) {
        return true;
    }
    false
}

fn enclosing_function_info<'a>(
    node: &'a oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> Option<(Option<String>, &'a oxc_semantic::AstNode<'a>)> {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return None; // root
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::Function(func) => {
                let name = func.id.as_ref().map(|id| id.name.as_str().to_string());
                // If name is None, check if parent is a MethodDefinition
                if name.is_none() {
                    let gp_id = nodes.parent_id(parent_id);
                    if gp_id != parent_id {
                        let gp = nodes.get_node(gp_id);
                        if let AstKind::MethodDefinition(method) = gp.kind() {
                            let key_text = &source[method.key.span().start as usize..method.key.span().end as usize];
                            return Some((Some(key_text.to_string()), parent));
                        }
                    }
                }
                // If still None, check for VariableDeclarator parent (function expression)
                if name.is_none() {
                    let gp_id = nodes.parent_id(parent_id);
                    if gp_id != parent_id {
                        let gp = nodes.get_node(gp_id);
                        if let AstKind::VariableDeclarator(decl) = gp.kind()
                            && let BindingPattern::BindingIdentifier(id) = &decl.id {
                                return Some((Some(id.name.as_str().to_string()), parent));
                            }
                    }
                }
                return Some((name, parent));
            }
            AstKind::ArrowFunctionExpression(_) => {
                // Try to resolve assigned name via VariableDeclarator
                let mut name = None;
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id {
                    let gp = nodes.get_node(gp_id);
                    if let AstKind::VariableDeclarator(decl) = gp.kind()
                        && let BindingPattern::BindingIdentifier(id) = &decl.id {
                            name = Some(id.name.as_str().to_string());
                        }
                }
                return Some((name, parent));
            }
            _ => {
                current_id = parent_id;
            }
        }
    }
}

fn is_in_handler_context(
    fn_node: &oxc_semantic::AstNode,
    name: Option<&str>,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    if let Some(n) = name
        && name_looks_like_handler(n) {
            return true;
        }

    // Check parameters for request-like names/types
    match fn_node.kind() {
        AstKind::Function(func) => {
            if params_look_like_handler(&func.params, source) {
                return true;
            }
        }
        AstKind::ArrowFunctionExpression(arrow) => {
            if params_look_like_handler(&arrow.params, source) {
                return true;
            }
        }
        _ => {}
    }

    // Check if inline route callback
    if is_inline_route_callback(fn_node, semantic, source) {
        return true;
    }

    false
}

fn params_look_like_handler(params: &FormalParameters, source: &str) -> bool {
    for param in &params.items {
        if let BindingPattern::BindingIdentifier(id) = &param.pattern {
            let name = id.name.as_str();
            if REQUEST_PARAM_NAMES.contains(&name) {
                return true;
            }
        }
        // Check type annotation
        if let Some(type_ann) = &param.type_annotation {
            let type_text: &str = &source[type_ann.span().start as usize..type_ann.span().end as usize];
            if type_text.contains("Request") || type_text.contains("NextApiRequest") {
                return true;
            }
        }
    }
    false
}

fn is_inline_route_callback(
    fn_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    _source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(fn_node.id());
    if parent_id == fn_node.id() {
        return false;
    }
    let parent = nodes.get_node(parent_id);

    // For arrow/function expression: parent might be the CallExpression's arguments
    // We need to find the CallExpression ancestor
    let call_id = match parent.kind() {
        AstKind::CallExpression(_) => parent_id,
        _ => {
            // Try grandparent (might be wrapped in Argument)
            let gp_id = nodes.parent_id(parent_id);
            if gp_id == parent_id {
                return false;
            }
            let gp = nodes.get_node(gp_id);
            match gp.kind() {
                AstKind::CallExpression(_) => gp_id,
                _ => return false,
            }
        }
    };

    let call_node = nodes.get_node(call_id);
    let AstKind::CallExpression(call) = call_node.kind() else {
        return false;
    };

    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };

    let method = member.property.name.as_str();
    ROUTE_VERBS.contains(&method)
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

    /// Run against a project where an HTTP API server framework is detected
    /// (Next.js) — the only context in which this rule fires.
    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            "t.ts",
            &crate::project::ProjectCtx::for_test_with_framework("nextjs"),
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn flags_schema_parse_in_internal_function() {
        let d = run("function computeTotal(input: unknown) { return Schema.parse(input); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("computeTotal"));
    }

    #[test]
    fn silent_in_non_http_server_project() {
        // Issue #1637: `shadcn` is a CLI tool that validates its `components.json`
        // config at the point of reading — that IS the boundary. With no HTTP API
        // framework detected, the rule must stay silent even for `schema.parse`.
        let src = "async function getRawConfig(cwd: string): Promise<RawConfig | null> { \
            const json = JSON.parse(await fs.readFile(configPath, 'utf-8')); \
            return rawConfigSchema.parse(json); }";
        let diagnostics = crate::rules::test_helpers::run_rule(&Check, src, "src/utils/get-config.ts");
        assert!(
            diagnostics.is_empty(),
            "CLI/library zod.parse must not fire without an HTTP API framework"
        );
    }

    #[test]
    fn fires_at_unvalidated_http_boundary_internal_helper() {
        // Negative-space guard: in a genuine HTTP API server project, a schema
        // `.parse(...)` buried in an internal helper still warns.
        let d = run("function getRawConfig(json: unknown) { return rawConfigSchema.parse(json); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getRawConfig"));
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
}

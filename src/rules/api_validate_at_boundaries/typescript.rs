//! For every `call_expression` whose callee is `<x>.parse` or
//! `<x>.safeParse`, walk up the AST to find the enclosing function. If
//! that function's name doesn't look like a handler / middleware,
//! emit a diagnostic.
//!
//! "Looks like a handler/middleware" heuristic:
//!   - name contains `handler`, `middleware`, `route`, `controller`
//!   - name starts with HTTP verb (`get`, `post`, `put`, `patch`, `delete`)
//!   - function has 2+ params where first param type mentions `Request`
//!     or one of the params is named `req`, `request`, `ctx`, `context`

use crate::diagnostic::{Diagnostic, Severity};

fn is_parse_call(callee: tree_sitter::Node, source: &[u8]) -> Option<&'static str> {
    if callee.kind() != "member_expression" {
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
    "route",
    "controller",
    "endpoint",
    "resolver",
];

const VERB_PREFIXES: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

fn name_looks_like_handler(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if HANDLER_KEYWORDS.iter().any(|k| lower.contains(k)) {
        return true;
    }
    // Verb prefix followed by an uppercase letter: getUser, postOrder, ...
    for v in VERB_PREFIXES {
        if lower.starts_with(v) && name.len() > v.len() {
            let next = name.as_bytes()[v.len()];
            if next.is_ascii_uppercase() {
                return true;
            }
        }
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
                    && REQUEST_PARAM_NAMES.contains(&name) {
                        return true;
                    }
        if let Some(type_ann) = child.child_by_field_name("type")
            && let Ok(text) = std::str::from_utf8(&source[type_ann.byte_range()])
                && (text.contains("Request") || text.contains("NextApiRequest")) {
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
                        && let Some(id) = gp.child_by_field_name("name") {
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
        && name_looks_like_handler(n) {
            return true;
        }
    // Inspect parameters.
    if let Some(params) = fn_node.child_by_field_name("parameters")
        && params_look_like_handler(params, source) {
            return true;
        }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
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
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
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
        assert!(run(
            "function userHandler(req: Request, res: Response) { Schema.parse(req.body); }"
        )
        .is_empty());
    }

    #[test]
    fn allows_parse_in_function_with_request_param() {
        assert!(run(
            "function run(req: Request, res: Response) { Schema.parse(req.body); }"
        )
        .is_empty());
    }

    #[test]
    fn allows_parse_at_module_level() {
        assert!(run("const config = ConfigSchema.parse(process.env);").is_empty());
    }

    #[test]
    fn allows_parse_in_verb_prefixed_function() {
        assert!(run("function getUser(req: Request) { return Schema.parse(req.body); }").is_empty());
    }
}

//! no-invalid-fetch-options AST backend — flag `fetch(...)` /
//! `new Request(...)` calls whose options object has a `body` while the
//! `method` is `GET` (default) or `HEAD`.
//!
//! Walks `call_expression` (for `fetch`) and `new_expression` (for
//! `new Request`) and inspects the second argument when it is an object
//! literal. A spread element (`...options`) without an explicit `method`
//! is considered safe — the spread may bring its own method.

use crate::diagnostic::{Diagnostic, Severity};

/// Lookup result for a property in an object literal.
enum PropValue<'a> {
    /// Property is present with this string-literal value (already unquoted).
    StringLiteral(&'a str),
    /// Property is present but value is not a string literal.
    Other,
    /// Property absent.
    Missing,
}

fn unquote(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

fn lookup_property<'a>(
    obj: tree_sitter::Node,
    source: &'a [u8],
    name: &str,
) -> PropValue<'a> {
    if obj.kind() != "object" {
        return PropValue::Missing;
    }
    let mut cursor = obj.walk();
    for prop in obj.named_children(&mut cursor) {
        if prop.kind() != "pair" {
            continue;
        }
        let Some(key) = prop.child_by_field_name("key") else {
            continue;
        };
        let Ok(raw_key) = key.utf8_text(source) else {
            continue;
        };
        if unquote(raw_key) != name {
            continue;
        }
        let Some(value) = prop.child_by_field_name("value") else {
            return PropValue::Other;
        };
        if value.kind() == "string"
            && let Ok(text) = value.utf8_text(source)
        {
            return PropValue::StringLiteral(unquote(text));
        }
        return PropValue::Other;
    }
    PropValue::Missing
}

fn has_spread(obj: tree_sitter::Node) -> bool {
    if obj.kind() != "object" {
        return false;
    }
    let mut cursor = obj.walk();
    obj.named_children(&mut cursor)
        .any(|c| c.kind() == "spread_element")
}

/// True if `value` is the literal `null` or `undefined`.
fn is_nullish_literal(prop: tree_sitter::Node, source: &[u8]) -> bool {
    if prop.kind() != "pair" {
        return false;
    }
    let Some(value) = prop.child_by_field_name("value") else {
        return false;
    };
    match value.kind() {
        "null" | "undefined" => true,
        "identifier" => value.utf8_text(source).unwrap_or("") == "undefined",
        _ => false,
    }
}

/// Find a property by name and return its full `pair` node (so callers can
/// inspect both key and value, including detecting `null`/`undefined`).
fn find_property<'a>(
    obj: tree_sitter::Node<'a>,
    source: &[u8],
    name: &str,
) -> Option<tree_sitter::Node<'a>> {
    if obj.kind() != "object" {
        return None;
    }
    let mut cursor = obj.walk();
    for prop in obj.named_children(&mut cursor) {
        if prop.kind() != "pair" {
            continue;
        }
        let Some(key) = prop.child_by_field_name("key") else {
            continue;
        };
        let Ok(raw_key) = key.utf8_text(source) else {
            continue;
        };
        if unquote(raw_key) == name {
            return Some(prop);
        }
    }
    None
}

/// Inspect an options object for the GET/HEAD + body violation. Returns
/// the offending method (`"GET"` or `"HEAD"`) when violation detected.
fn detect_violation(obj: tree_sitter::Node, source: &[u8]) -> Option<&'static str> {
    let body_pair = find_property(obj, source, "body")?;
    if is_nullish_literal(body_pair, source) {
        return None;
    }

    let method = match lookup_property(obj, source, "method") {
        PropValue::StringLiteral(s) => s.to_ascii_uppercase(),
        PropValue::Other => return None,
        PropValue::Missing => {
            // No method: default GET — but a spread might bring one.
            if has_spread(obj) {
                return None;
            }
            "GET".to_string()
        }
    };

    if method == "GET" {
        Some("GET")
    } else if method == "HEAD" {
        Some("HEAD")
    } else {
        None
    }
}

/// True if `callee` is the bare identifier `fetch`.
fn is_fetch_callee(callee: tree_sitter::Node, source: &[u8]) -> bool {
    callee.kind() == "identifier" && callee.utf8_text(source).unwrap_or("") == "fetch"
}

/// True if `constructor` of a `new_expression` is `Request`.
fn is_request_constructor(ctor: tree_sitter::Node, source: &[u8]) -> bool {
    ctor.kind() == "identifier" && ctor.utf8_text(source).unwrap_or("") == "Request"
}

crate::ast_check! { on ["call_expression", "new_expression"] => |node, source, ctx, diagnostics|
    let kind = node.kind();
    let (relevant, args_field) = match kind {
        "call_expression" => {
            let Some(callee) = node.child_by_field_name("function") else { return };
            (is_fetch_callee(callee, source), "arguments")
        }
        "new_expression" => {
            let Some(ctor) = node.child_by_field_name("constructor") else { return };
            (is_request_constructor(ctor, source), "arguments")
        }
        _ => return,
    };
    if !relevant {
        return;
    }
    let Some(args) = node.child_by_field_name(args_field) else { return };
    let mut cursor = args.walk();
    let Some(opts) = args.named_children(&mut cursor).nth(1) else { return };
    if opts.kind() != "object" {
        return;
    }
    if let Some(method) = detect_violation(opts, source) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "no-invalid-fetch-options",
            format!("`body` is not allowed when method is \"{}\".", method),
            Severity::Error,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_body_with_default_get() {
        let code = r#"fetch(url, { body: 'hello' });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("GET"));
    }

    #[test]
    fn flags_body_with_explicit_get() {
        let code = r#"fetch(url, { method: 'GET', body: 'hello' });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_body_with_head() {
        let code = r#"fetch(url, { method: 'HEAD', body: 'hello' });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("HEAD"));
    }

    #[test]
    fn flags_new_request_with_body_get() {
        let code = r#"new Request(url, { body: 'hello' });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_body_with_post() {
        let code = r#"fetch(url, { method: 'POST', body: JSON.stringify(data) });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_body_null() {
        let code = r#"fetch(url, { body: null });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_body_undefined() {
        let code = r#"fetch(url, { body: undefined });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_spread_without_method() {
        let code = r#"fetch(url, { ...options, body: 'hello' });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_multiline_fetch() {
        let code = r#"
fetch(url, {
    body: JSON.stringify(data),
    method: 'GET',
});
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }
}

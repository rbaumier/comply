//! Detect `call_expression` nodes whose callee is a `member_expression`
//! naming one of the HTML-parsing DOM methods, and whose HTML argument
//! is not a static string literal.
//!
//! Target methods and the argument index that carries the HTML payload:
//! - `insertAdjacentHTML(position, html)` → arg 1
//! - `document.write(html)` → arg 0
//! - `document.writeln(html)` → arg 0
//! - `setHTMLUnsafe(html)` → arg 0
//! - `createContextualFragment(html)` → arg 0 (on `Range`)
//!
//! An argument is "safe" when it is a string literal or a template
//! literal without any `${…}` interpolation. Everything else
//! (identifiers, calls, concatenations, interpolated templates) is
//! flagged as a potential XSS sink.

use crate::diagnostic::{Diagnostic, Severity};

/// True when `node` is a template literal with no `${…}` substitutions.
fn is_static_template(node: tree_sitter::Node) -> bool {
    if node.kind() != "template_string" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "template_substitution" {
            return false;
        }
    }
    true
}

/// True when `node` is a safe, fully-static string expression.
fn is_static_string(node: tree_sitter::Node) -> bool {
    node.kind() == "string" || is_static_template(node)
}

/// Returns the 0-based argument index carrying the HTML payload for
/// the given method name, or `None` if the method is not targeted.
fn html_arg_index(method: &str) -> Option<usize> {
    match method {
        "insertAdjacentHTML" => Some(1),
        "write" | "writeln" | "setHTMLUnsafe" | "createContextualFragment" => Some(0),
        _ => None,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else { return; };
    let method = prop.utf8_text(source).unwrap_or("");
    let Some(idx) = html_arg_index(method) else { return; };

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let mut cursor = args.walk();
    let named: Vec<tree_sitter::Node> = args.named_children(&mut cursor).collect();
    let Some(html_arg) = named.get(idx) else { return; };
    if is_static_string(*html_arg) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-unsanitized-method".into(),
        message: format!(
            "Calling `{method}` with a non-literal HTML argument is an XSS vector — avoid dynamic HTML injection, or sanitize input first."
        ),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_insert_adjacent_html_variable() {
        let src = "el.insertAdjacentHTML('beforeend', userInput);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_document_write_concat() {
        let src = "document.write('<p>' + name + '</p>');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_document_writeln_template() {
        let src = "document.writeln(`<p>${name}</p>`);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_set_html_unsafe_variable() {
        let src = "el.setHTMLUnsafe(userInput);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_create_contextual_fragment_variable() {
        let src = "range.createContextualFragment(userInput);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_insert_adjacent_html_literal() {
        let src = "el.insertAdjacentHTML('beforeend', '<p>static</p>');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_document_write_literal() {
        let src = "document.write('<p>static</p>');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_set_html_unsafe_static_template() {
        let src = "el.setHTMLUnsafe(`<p>static</p>`);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unrelated_method() {
        let src = "el.appendChild(child);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_bare_identifier_call() {
        // Callee is not a member_expression — skip.
        let src = "write(userInput);";
        assert!(run_on(src).is_empty());
    }
}

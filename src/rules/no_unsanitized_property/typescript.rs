//! Detect `el.innerHTML = <non-literal>`, `el.outerHTML = …`, `el.srcdoc = …`.
//!
//! Only plain `=` assignments are flagged. Compound operators (`+=`, `*=`, …)
//! are ignored because they are not the typical XSS sink pattern we target
//! here and tree-sitter exposes them as `augmented_assignment_expression`.
//!
//! The right-hand side is considered safe when it is a string literal or a
//! template literal without any `${…}` interpolation. Everything else
//! (identifiers, calls, concatenations, interpolated templates) is flagged.

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

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
    let _ = source;
    let Some(left) = node.child_by_field_name("left") else { return };
    if left.kind() != "member_expression" {
        return;
    }
    let Some(prop) = left.child_by_field_name("property") else { return };
    let prop_name = prop.utf8_text(source).unwrap_or("");
    if !matches!(prop_name, "innerHTML" | "outerHTML" | "srcdoc") {
        return;
    }

    let Some(right) = node.child_by_field_name("right") else { return };
    if is_static_string(right) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-unsanitized-property".into(),
        message: format!(
            "Assigning a non-literal value to `{prop_name}` is an XSS vector — use textContent or sanitize the HTML first."
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
    fn flags_inner_html_variable() {
        let src = "el.innerHTML = userInput;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_outer_html_call() {
        let src = "el.outerHTML = getHtml();";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_srcdoc_concat() {
        let src = "frame.srcdoc = \"<p>\" + name + \"</p>\";";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_interpolated_template() {
        let src = "el.innerHTML = `<p>${name}</p>`;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_string_literal() {
        let src = "el.innerHTML = \"<p>static</p>\";";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_template() {
        let src = "el.innerHTML = `<p>static</p>`;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_compound_assignment() {
        // `+=` parses as `augmented_assignment_expression`, not
        // `assignment_expression`, so it should not be flagged.
        let src = "el.innerHTML += extra;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unrelated_property() {
        let src = "el.textContent = userInput;";
        assert!(run_on(src).is_empty());
    }
}

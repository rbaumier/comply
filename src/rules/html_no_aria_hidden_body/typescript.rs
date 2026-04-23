//! html-no-aria-hidden-body AST backend.

use crate::diagnostic::{Diagnostic, Severity};

/// True when the `aria-hidden` attribute effectively equals `true`.
/// Accepts `aria-hidden` (no value, implicit true), `aria-hidden="true"`,
/// and `aria-hidden={true}`.
fn is_aria_hidden_true(attr: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(val) = crate::rules::jsx::jsx_attribute_value(attr) else {
        // No value: shorthand form is truthy.
        return true;
    };
    let Ok(text) = val.utf8_text(source) else {
        return false;
    };
    let trimmed = text.trim();
    let unquoted = trimmed.trim_matches(|c| c == '"' || c == '\'');
    if unquoted == "true" {
        return true;
    }
    // JSX expression {true}
    if let Some(inner) = trimmed.strip_prefix('{').and_then(|s| s.strip_suffix('}'))
        && inner.trim() == "true"
    {
        return true;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_opening_element" && kind != "jsx_self_closing_element" {
        return;
    }

    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "body" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        if crate::rules::jsx::jsx_attribute_name(child, source) != Some("aria-hidden") {
            continue;
        }
        if !is_aria_hidden_true(child, source) {
            continue;
        }
        let pos = child.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "html-no-aria-hidden-body".into(),
            message: "`aria-hidden=\"true\"` on `<body>` hides the entire page from assistive tech.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_body_aria_hidden_true_string() {
        assert_eq!(run(r#"const x = <body aria-hidden="true" />;"#).len(), 1);
    }

    #[test]
    fn flags_body_aria_hidden_expr() {
        assert_eq!(run(r#"const x = <body aria-hidden={true} />;"#).len(), 1);
    }

    #[test]
    fn allows_body_aria_hidden_false() {
        assert!(run(r#"const x = <body aria-hidden="false" />;"#).is_empty());
    }

    #[test]
    fn allows_aria_hidden_on_div() {
        assert!(run(r#"const x = <div aria-hidden="true" />;"#).is_empty());
    }

    #[test]
    fn allows_plain_body() {
        assert!(run(r#"const x = <body />;"#).is_empty());
    }
}

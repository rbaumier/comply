//! react-jsx-no-script-url AST backend.
//!
//! Flags `href="javascript:..."` or `href={'javascript:...'}` in JSX.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match JSX attribute nodes named "href"
    if node.kind() != "jsx_attribute" {
        return;
    }

    let Some(name_node) = node.child(0) else { return };
    let Ok(name_text) = name_node.utf8_text(source) else { return };
    if !name_text.eq_ignore_ascii_case("href") {
        return;
    }

    // Get the attribute value
    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };

    let value_text = match value_node.kind() {
        // href="javascript:..."
        "string" => {
            let Ok(t) = value_node.utf8_text(source) else { return };
            t.to_string()
        }
        // href={'javascript:...'} or href={`javascript:...`}
        "jsx_expression" => {
            let Ok(t) = value_node.utf8_text(source) else { return };
            t.to_string()
        }
        _ => return,
    };

    let lower = value_text.to_ascii_lowercase();
    if lower.contains("javascript:") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-jsx-no-script-url".into(),
            message: "`javascript:` URLs are an XSS vector. Use an \
                      `onClick` handler instead."
                .into(),
            severity: Severity::Error,
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
    fn flags_javascript_href() {
        let src = r#"const x = <a href="javascript:alert('xss')">click</a>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_javascript_href_expression() {
        let src = r#"const x = <a href={'javascript:void(0)'}>click</a>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_javascript_href_template() {
        let src = r#"const x = <a href={`javascript:alert(1)`}>click</a>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_normal_href() {
        let src = r#"const x = <a href="https://example.com">click</a>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_hash_href() {
        let src = r##"const x = <a href="#">click</a>;"##;
        assert!(run(src).is_empty());
    }
}

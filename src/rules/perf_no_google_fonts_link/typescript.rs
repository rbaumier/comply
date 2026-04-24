//! AST backend — flags JSX `<link>` whose `href` points at Google Fonts.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_self_closing_element" && node.kind() != "jsx_opening_element" {
        return;
    }
    if jsx_element_tag_name(node, source) != Some("link") {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        if jsx_attribute_name(child, source) != Some("href") { continue; }
        let Some(val) = jsx_attribute_string_value(child, source) else { continue };
        if val.contains("fonts.googleapis.com") || val.contains("fonts.gstatic.com") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "Avoid loading fonts from `fonts.googleapis.com` — self-host them to cut a third-party handshake.".into(),
                Severity::Warning,
            ));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_google_fonts_link() {
        let code = r#"const x = <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter" />;"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_gstatic() {
        let code = r#"const x = <link rel="preconnect" href="https://fonts.gstatic.com" />;"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_self_hosted_link() {
        let code = r#"const x = <link rel="stylesheet" href="/fonts/inter.css" />;"#;
        assert!(run(code).is_empty());
    }
}

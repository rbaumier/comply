//! Detects `<link rel="stylesheet" ... />` — should be a CSS import instead.
//! Skips Google Fonts URLs (covered by `next-no-font-link`).

use crate::diagnostic::{Diagnostic, Severity};

const FONT_HOSTS: &[&str] = &["fonts.googleapis.com", "fonts.gstatic.com"];

fn get_jsx_attribute_string<'a>(element: tree_sitter::Node<'a>, source: &'a [u8], attr_name: &str) -> Option<&'a str> {
    let mut cursor = element.walk();
    element.children(&mut cursor).find_map(|child| {
        if child.kind() != "jsx_attribute" {
            return None;
        }
        if crate::rules::jsx::jsx_attribute_name(child, source) != Some(attr_name) {
            return None;
        }
        let val = crate::rules::jsx::jsx_attribute_value(child)?;
        if val.kind() != "string" {
            return None;
        }
        let raw = val.utf8_text(source).ok()?;
        Some(raw.trim_matches(|c| c == '"' || c == '\''))
    })
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let tag_name = node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");
    if tag_name != "link" {
        return;
    }

    if get_jsx_attribute_string(node, source, "rel") != Some("stylesheet") {
        return;
    }

    // Defer to next-no-font-link for Google Fonts URLs.
    if let Some(href) = get_jsx_attribute_string(node, source, "href") {
        if FONT_HOSTS.iter().any(|host| href.contains(host)) {
            return;
        }
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`<link rel=\"stylesheet\">` — import CSS directly so Next.js can bundle and optimize it.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_stylesheet_link() {
        let diags = run(r#"function App() { return <link rel="stylesheet" href="/styles.css" />; }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_stylesheet_link_no_href() {
        let diags = run(r#"function App() { return <link rel="stylesheet" />; }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_stylesheet_link_in_opening_tag() {
        let diags = run(r#"function App() { return <link rel="stylesheet" href="/a.css"></link>; }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_non_stylesheet_rel() {
        assert!(run(r#"function App() { return <link rel="canonical" href="/" />; }"#).is_empty());
    }

    #[test]
    fn allows_google_fonts_stylesheet_link() {
        // Reported by next-no-font-link, not this rule.
        assert!(run(r#"function App() { return <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter" />; }"#).is_empty());
    }

    #[test]
    fn ignores_other_tags() {
        assert!(run(r#"function App() { return <a rel="stylesheet" href="/x.css">x</a>; }"#).is_empty());
    }
}

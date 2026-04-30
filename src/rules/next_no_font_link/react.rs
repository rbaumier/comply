//! Detects `<link href="...fonts.googleapis.com..." />` and
//! `<link href="...fonts.gstatic.com..." />` — should use `next/font`.

use crate::diagnostic::{Diagnostic, Severity};

const FONT_HOSTS: &[&str] = &["fonts.googleapis.com", "fonts.gstatic.com"];

fn get_jsx_attribute_string<'a>(
    element: tree_sitter::Node<'a>,
    source: &'a [u8],
    attr_name: &str,
) -> Option<&'a str> {
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

    let Some(href) = get_jsx_attribute_string(node, source, "href") else { return };
    if !FONT_HOSTS.iter().any(|host| href.contains(host)) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "Loading Google Fonts via `<link>` — use `next/font` for self-hosting and zero layout shift.".into(),
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
    fn flags_googleapis_font_link() {
        let diags = run(
            r#"function App() { return <link href="https://fonts.googleapis.com/css2?family=Inter" rel="stylesheet" />; }"#,
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_gstatic_font_link() {
        let diags = run(
            r#"function App() { return <link href="https://fonts.gstatic.com/s/inter/v12/abc.woff2" rel="preload" as="font" />; }"#,
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_googleapis_in_opening_tag() {
        let diags = run(
            r#"function App() { return <link href="https://fonts.googleapis.com/css?family=Roboto" rel="stylesheet"></link>; }"#,
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_non_font_link() {
        assert!(
            run(r#"function App() { return <link href="/styles.css" rel="stylesheet" />; }"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_link_without_href() {
        assert!(run(r#"function App() { return <link rel="canonical" />; }"#).is_empty());
    }

    #[test]
    fn ignores_anchor_tag() {
        assert!(run(r#"function App() { return <a href="https://fonts.googleapis.com/css?family=Inter">x</a>; }"#).is_empty());
    }
}

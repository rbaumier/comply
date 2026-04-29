//! AST backend — flags JSX `<link rel="stylesheet">` elements with no
//! `media` attribute.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] prefilter = ["stylesheet"] => |node, source, ctx, diagnostics|
    if jsx_element_tag_name(node, source) != Some("link") {
        return;
    }

    let mut rel: Option<String> = None;
    let mut has_media = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = jsx_attribute_name(child, source) else { continue };
        match name {
            "rel" => rel = jsx_attribute_string_value(child, source).map(str::to_owned),
            "media" => has_media = true,
            _ => {}
        }
    }

    if rel.as_deref() != Some("stylesheet") { return; }
    if has_media { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`<link rel=\"stylesheet\">` without a `media` attribute blocks first paint — add `media=\"...\"` so the browser can defer non-critical CSS.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_stylesheet_without_media() {
        assert_eq!(
            run(r#"const x = <link rel="stylesheet" href="/a.css" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_stylesheet_with_media() {
        assert!(
            run(r#"const x = <link rel="stylesheet" href="/a.css" media="print" />;"#).is_empty()
        );
    }

    #[test]
    fn ignores_non_stylesheet_link() {
        assert!(
            run(r#"const x = <link rel="preload" as="style" href="/a.css" />;"#).is_empty()
        );
    }
}

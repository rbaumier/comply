//! AST backend — flags `<img>` legacy raster formats that aren't wrapped in
//! a `<picture>` and don't declare a `srcset` with an alternate format.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name};

fn has_legacy_extension(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    let bare = lower.split(['?', '#']).next().unwrap_or(&lower);
    bare.ends_with(".jpg") || bare.ends_with(".jpeg") || bare.ends_with(".png")
}

fn is_inside_picture(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "jsx_element"
            && let Some(opening) = n.child(0)
            && opening.kind() == "jsx_opening_element"
            && jsx_element_tag_name(opening, source) == Some("picture")
        {
            return true;
        }
        cur = n.parent();
    }
    false
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] prefilter = ["img"] => |node, source, ctx, diagnostics|
    if jsx_element_tag_name(node, source) != Some("img") {
        return;
    }

    let mut src_val: Option<String> = None;
    let mut has_srcset = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = jsx_attribute_name(child, source) else { continue };
        match name {
            "src" => {
                src_val = jsx_attribute_string_value(child, source).map(str::to_owned);
            }
            "srcSet" | "srcset" => {
                has_srcset = true;
            }
            _ => {}
        }
    }

    let Some(src) = src_val else { return };
    if !has_legacy_extension(&src) {
        return;
    }
    if has_srcset {
        return;
    }
    if is_inside_picture(node, source) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`<img src=\"...jpg|.png|.jpeg\">` should offer a WebP/AVIF alternative via `<picture>` or `srcset`.".into(),
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
    fn flags_plain_jpg() {
        assert_eq!(run(r#"const x = <img src="hero.jpg" />;"#).len(), 1);
    }

    #[test]
    fn flags_plain_png() {
        assert_eq!(run(r#"const x = <img src="logo.png" alt="" />;"#).len(), 1);
    }

    #[test]
    fn allows_webp() {
        assert!(run(r#"const x = <img src="hero.webp" />;"#).is_empty());
    }

    #[test]
    fn allows_img_with_srcset() {
        assert!(
            run(r#"const x = <img src="hero.jpg" srcSet="hero.webp 1x, hero.avif 2x" />;"#).is_empty()
        );
    }

    #[test]
    fn allows_img_inside_picture() {
        let code = r#"
            const x = (
                <picture>
                    <source type="image/webp" srcSet="hero.webp" />
                    <img src="hero.jpg" />
                </picture>
            );
        "#;
        assert!(run(code).is_empty());
    }
}

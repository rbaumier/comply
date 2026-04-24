//! AST backend — walks JSX `<img>` elements and checks size hints / conflicting attrs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name};

/// Threshold (in pixels) above which an image is considered "hero-sized"
/// and should declare `fetchpriority="high"`. 600px covers typical hero
/// banners while skipping avatars/thumbnails.
const HERO_PIXEL_THRESHOLD: u32 = 600;

fn parse_dim(val: &str) -> Option<u32> {
    // strip trailing unit ("px") and whitespace
    let trimmed = val.trim().trim_end_matches("px").trim();
    trimmed.parse::<u32>().ok()
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_self_closing_element" && node.kind() != "jsx_opening_element" {
        return;
    }
    if jsx_element_tag_name(node, source) != Some("img") {
        return;
    }

    let mut fetchpriority: Option<String> = None;
    let mut loading: Option<String> = None;
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = jsx_attribute_name(child, source) else { continue };
        match name {
            "fetchpriority" => {
                fetchpriority = jsx_attribute_string_value(child, source).map(str::to_owned);
            }
            "loading" => {
                loading = jsx_attribute_string_value(child, source).map(str::to_owned);
            }
            "width" => {
                if let Some(v) = jsx_attribute_string_value(child, source).and_then(parse_dim) {
                    width = Some(v);
                }
            }
            "height" => {
                if let Some(v) = jsx_attribute_string_value(child, source).and_then(parse_dim) {
                    height = Some(v);
                }
            }
            _ => {}
        }
    }

    // Case 1: conflicting fetchpriority="high" + loading="lazy"
    if fetchpriority.as_deref() == Some("high") && loading.as_deref() == Some("lazy") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`<img>` with `fetchpriority=\"high\"` must not also set `loading=\"lazy\"` — they contradict each other.".into(),
            Severity::Warning,
        ));
        return;
    }

    // Case 2: hero-sized img without fetchpriority="high"
    let is_hero = width.is_some_and(|w| w >= HERO_PIXEL_THRESHOLD)
        || height.is_some_and(|h| h >= HERO_PIXEL_THRESHOLD);
    if is_hero && fetchpriority.as_deref() != Some("high") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Hero-sized `<img>` should declare `fetchpriority=\"high\"` so the browser starts fetching it early.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_hero_without_fetchpriority() {
        assert_eq!(run(r#"const x = <img src="h.jpg" width="1200" height="800" />;"#).len(), 1);
    }

    #[test]
    fn flags_conflicting_high_and_lazy() {
        assert_eq!(
            run(r#"const x = <img src="h.jpg" fetchpriority="high" loading="lazy" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_small_img_without_fetchpriority() {
        assert!(run(r#"const x = <img src="a.jpg" width="48" height="48" />;"#).is_empty());
    }

    #[test]
    fn allows_hero_with_fetchpriority_high() {
        assert!(run(r#"const x = <img src="h.jpg" width="1200" fetchpriority="high" />;"#).is_empty());
    }
}

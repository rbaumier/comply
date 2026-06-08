//! AST backend — walks JSX `<img>` elements and checks size hints / conflicting attrs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{
    jsx_attribute_name, jsx_attribute_string_value, jsx_attribute_value, jsx_element_tag_name,
};

fn parse_dim(val: &str) -> Option<u32> {
    // strip trailing unit ("px") and whitespace
    let trimmed = val.trim().trim_end_matches("px").trim();
    trimmed.parse::<u32>().ok()
}

/// Extract a numeric value from a JSX attribute that uses an expression
/// container, e.g. `width={1200}`. Returns `None` for non-numeric or
/// non-expression values.
fn jsx_attribute_numeric_expr(attr: tree_sitter::Node<'_>, source: &[u8]) -> Option<u32> {
    let val = jsx_attribute_value(attr)?;
    if val.kind() != "jsx_expression" {
        return None;
    }
    let mut cursor = val.walk();
    for child in val.named_children(&mut cursor) {
        if child.kind() == "number" {
            let text = child.utf8_text(source).ok()?;
            return text.trim().parse::<u32>().ok();
        }
    }
    None
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] prefilter = ["img"] => |node, source, ctx, diagnostics|
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
                } else if let Some(v) = jsx_attribute_numeric_expr(child, source) {
                    width = Some(v);
                }
            }
            "height" => {
                if let Some(v) = jsx_attribute_string_value(child, source).and_then(parse_dim) {
                    height = Some(v);
                } else if let Some(v) = jsx_attribute_numeric_expr(child, source) {
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
    let hero_threshold = ctx.config.threshold("perf-img-fetchpriority-high", "hero_pixel_threshold", ctx.lang) as u32;
    let is_hero = width.is_some_and(|w| w >= hero_threshold)
        || height.is_some_and(|h| h >= hero_threshold);
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_hero_without_fetchpriority() {
        assert_eq!(
            run(r#"const x = <img src="h.jpg" width="1200" height="800" />;"#).len(),
            1
        );
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
        assert!(
            run(r#"const x = <img src="h.jpg" width="1200" fetchpriority="high" />;"#).is_empty()
        );
    }

    #[test]
    fn flags_hero_with_numeric_expression_dimensions() {
        // width={1200} is a JSX expression container around a number,
        // not a string attribute. The rule must still detect hero size.
        assert_eq!(
            run(r#"const x = <img src="h.jpg" width={1200} height={800} />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_small_img_with_numeric_expression_dimensions() {
        assert!(run(r#"const x = <img src="a.jpg" width={48} height={48} />;"#).is_empty());
    }
}

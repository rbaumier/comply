use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{
    jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name,
};

const HEADING_TAGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];
const LARGE_SIZES: &[&str] = &[
    "text-4xl", "text-5xl", "text-6xl", "text-7xl", "text-8xl", "text-9xl",
];
const BREAKPOINTS: &[&str] = &["sm:", "md:", "lg:", "xl:", "2xl:"];

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_opening_element" && kind != "jsx_self_closing_element" { return; }

    let Some(tag) = jsx_element_tag_name(node, source) else { return };
    let lower = tag.to_ascii_lowercase();
    if !HEADING_TAGS.contains(&lower.as_str()) { return; }

    let mut class_value: Option<&str> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(attr_name) = jsx_attribute_name(child, source) else { continue };
        if attr_name == "className" || attr_name == "class" {
            class_value = jsx_attribute_string_value(child, source);
            break;
        }
    }
    let Some(classes) = class_value else { return };

    let mut has_large_base = false;
    let mut has_responsive_text = false;
    for tok in classes.split_whitespace() {
        // Responsive text variant like `md:text-4xl` or `sm:text-sm`.
        if BREAKPOINTS.iter().any(|bp| tok.starts_with(bp)) {
            let after = tok.split_once(':').map(|x| x.1).unwrap_or("");
            if after.starts_with("text-") && !after.starts_with("text-[") {
                has_responsive_text = true;
            }
            continue;
        }
        if LARGE_SIZES.contains(&tok) {
            has_large_base = true;
        }
    }

    if has_large_base && !has_responsive_text {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Large heading size without a responsive variant — add `sm:text-*` / `md:text-*` so it scales on mobile.".into(),
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
    fn flags_h1_text_4xl_no_responsive() {
        assert_eq!(run(r#"export const A = () => <h1 className="text-4xl" />;"#).len(), 1);
    }

    #[test]
    fn flags_h2_text_6xl_no_responsive() {
        assert_eq!(run(r#"export const A = () => <h2 className="font-bold text-6xl" />;"#).len(), 1);
    }

    #[test]
    fn allows_responsive_pair() {
        assert!(run(r#"export const A = () => <h1 className="text-2xl md:text-4xl" />;"#).is_empty());
    }

    #[test]
    fn ignores_small_heading() {
        assert!(run(r#"export const A = () => <h1 className="text-xl" />;"#).is_empty());
    }

    #[test]
    fn ignores_non_heading_div() {
        assert!(run(r#"export const A = () => <div className="text-4xl" />;"#).is_empty());
    }
}

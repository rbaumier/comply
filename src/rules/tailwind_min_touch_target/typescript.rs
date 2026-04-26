use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{
    jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name,
};

const INTERACTIVE_TAGS: &[&str] = &["button", "a"];

/// Parse the numeric value from a spacing utility like `px-2`, `py-1`, `p-0`.
/// Returns the raw scale number (not px).
fn parse_spacing_value(tok: &str, prefix: &str) -> Option<u32> {
    let rest = tok.strip_prefix(prefix)?;
    rest.parse::<u32>().ok()
}

/// Does this className explicitly set an adequate height/width?
fn has_explicit_size(classes: &str) -> bool {
    classes.split_whitespace().any(|tok| {
        let base = tok.rsplit(':').next().unwrap_or(tok);
        for prefix in &["h-", "w-", "min-h-", "min-w-", "size-"] {
            if let Some(rest) = base.strip_prefix(prefix) {
                // Full/screen/auto and numeric >= 11 (44px) are fine.
                if rest == "full" || rest == "screen" {
                    return true;
                }
                if let Ok(n) = rest.parse::<u32>()
                    && n >= 11 { return true; }
            }
        }
        false
    })
}

/// Roughly: is the padding too small for a touch target? Tailwind scale `1`
/// is 4px, `2` is 8px, `3` is 12px — so vertical padding < 3 AND horizontal
/// padding < 3 means the target is under 24px even with single-line text.
fn padding_too_small(classes: &str) -> bool {
    let mut py = u32::MAX;
    let mut px = u32::MAX;

    for tok in classes.split_whitespace() {
        let base = tok.rsplit(':').next().unwrap_or(tok);
        if let Some(v) = parse_spacing_value(base, "p-") {
            py = py.min(v);
            px = px.min(v);
        }
        if let Some(v) = parse_spacing_value(base, "py-") {
            py = py.min(v);
        }
        if let Some(v) = parse_spacing_value(base, "px-") {
            px = px.min(v);
        }
        if let Some(v) = parse_spacing_value(base, "pt-") {
            py = py.min(v);
        }
        if let Some(v) = parse_spacing_value(base, "pb-") {
            py = py.min(v);
        }
    }

    // If we saw any padding and both axes are tiny (<3 = <12px).
    py != u32::MAX && px != u32::MAX && py < 3 && px < 3
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = jsx_element_tag_name(node, source) else { return };
    let lower = tag.to_ascii_lowercase();

    let mut class_value: Option<&str> = None;
    let mut is_role_button = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(attr_name) = jsx_attribute_name(child, source) else { continue };
        match attr_name {
            "className" | "class" => class_value = jsx_attribute_string_value(child, source),
            "role" => {
                if jsx_attribute_string_value(child, source) == Some("button") {
                    is_role_button = true;
                }
            }
            _ => {}
        }
    }

    let interactive = INTERACTIVE_TAGS.contains(&lower.as_str()) || is_role_button;
    if !interactive { return; }

    let classes = class_value.unwrap_or("");
    if has_explicit_size(classes) { return; }
    if !padding_too_small(classes) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Interactive element below the ~44x44px touch target (WCAG 2.5.5). Use `h-11` + sufficient padding, or `size-11` for icon buttons.".into(),
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
    fn flags_small_button() {
        assert_eq!(run(r#"export const A = () => <button className="px-2 py-1 text-xs" />;"#).len(), 1);
    }

    #[test]
    fn flags_tiny_anchor() {
        assert_eq!(run(r#"export const A = () => <a className="p-1" />;"#).len(), 1);
    }

    #[test]
    fn allows_explicit_height() {
        assert!(run(r#"export const A = () => <button className="h-11 px-2 py-1" />;"#).is_empty());
    }

    #[test]
    fn allows_generous_padding() {
        assert!(run(r#"export const A = () => <button className="px-4 py-3" />;"#).is_empty());
    }
}

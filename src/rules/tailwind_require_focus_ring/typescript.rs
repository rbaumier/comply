use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name};

const INTERACTIVE_TAGS: &[&str] = &["button", "a", "input", "select", "textarea"];

fn has_focus_ring(classes: &str) -> bool {
    // `focus:outline-none` / `focus:outline-0` REMOVE the focus indicator
    // rather than provide one. They must not count as a valid ring even
    // though they share the `focus:outline-` prefix.
    const OUTLINE_REMOVERS: &[&str] = &[
        "focus:outline-none",
        "focus:outline-0",
        "focus-visible:outline-none",
        "focus-visible:outline-0",
    ];
    classes.split_whitespace().any(|tok| {
        if OUTLINE_REMOVERS.contains(&tok) {
            return false;
        }
        tok.starts_with("focus:ring")
            || tok.starts_with("focus-visible:ring")
            || tok.starts_with("focus:outline")
            || tok.starts_with("focus-visible:outline")
            || tok.starts_with("focus:border-")
            || tok.starts_with("focus-visible:border-")
    })
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    // shadcn/ui primitives handle focus indicators internally.
    let path_str = ctx.path.to_str().unwrap_or("");
    if path_str.contains("/components/ui/") || path_str.contains("/lib/ui/") { return; }

    let Some(tag) = jsx_element_tag_name(node, source) else { return };
    // PascalCase = React component — focus ring may be baked into the component.
    if tag.as_bytes().first().is_some_and(|b| b.is_ascii_uppercase()) { return; }
    let lower = tag.to_ascii_lowercase();

    // Collect className + role from attributes in one pass.
    let mut class_value: Option<&str> = None;
    let mut is_role_button = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(attr_name) = jsx_attribute_name(child, source) else { continue };
        match attr_name {
            "className" | "class" => {
                class_value = jsx_attribute_string_value(child, source);
            }
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
    if has_focus_ring(classes) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Interactive element missing a `focus:ring-*` class — keyboard users need a visible focus indicator.".into(),
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
    fn flags_button_without_focus_ring() {
        assert_eq!(
            run(r#"export const A = () => <button className="px-4" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_role_button_without_focus_ring() {
        assert_eq!(
            run(r#"export const A = () => <div role="button" className="px-4" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_button_with_focus_ring() {
        assert!(
            run(r#"export const A = () => <button className="px-4 focus:ring-2" />;"#).is_empty()
        );
    }

    #[test]
    fn allows_input_with_focus_visible_ring() {
        assert!(
            run(r#"export const A = () => <input className="focus-visible:ring-2" />;"#).is_empty()
        );
    }

    #[test]
    fn ignores_non_interactive_div() {
        assert!(run(r#"export const A = () => <div className="px-4" />;"#).is_empty());
    }

    #[test]
    fn flags_focus_outline_none_alone() {
        // outline-none REMOVES the focus indicator — must not count as a ring.
        assert_eq!(
            run(r#"export const A = () => <button className="focus:outline-none" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_focus_outline_0_alone() {
        assert_eq!(
            run(r#"export const A = () => <button className="focus-visible:outline-0" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_outline_none_paired_with_ring() {
        // The recommended pattern: outline-none + a real ring.
        assert!(
            run(
                r#"export const A = () => <button className="focus:outline-none focus:ring-2" />;"#
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_focus_visible_border_ring() {
        assert!(
            run(r#"export const A = () => <button className="focus-visible:border-ring" />;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_bare_focus_visible_outline() {
        assert!(
            run(r#"export const A = () => <button className="focus-visible:outline" />;"#)
                .is_empty()
        );
    }

    #[test]
    fn skips_pascal_case_components() {
        assert!(run(r#"export const A = () => <Button className="px-4" />;"#).is_empty());
    }

    #[test]
    fn skips_shadcn_ui_components() {
        use crate::rules::test_helpers::run_ts_with_path;
        let src = r#"export const A = <button className="px-4" />;"#;
        let d = run_ts_with_path(src, &Check, "src/components/ui/sidebar.tsx");
        assert!(d.is_empty());
    }
}

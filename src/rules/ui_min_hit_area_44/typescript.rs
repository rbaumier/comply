//! Flag `<button|a|input>` elements whose Tailwind className forces a
//! sub-44-pixel footprint (e.g. `h-4 w-4`, `size-3`).

use crate::diagnostic::{Diagnostic, Severity};

const TINY_SIZE_TOKENS: &[&str] = &[
    "h-0", "h-1", "h-2", "h-3", "h-4", "h-5", "h-6", "h-7", "h-8", "h-9", "h-10",
    "w-0", "w-1", "w-2", "w-3", "w-4", "w-5", "w-6", "w-7", "w-8", "w-9", "w-10",
    "size-0", "size-1", "size-2", "size-3", "size-4", "size-5", "size-6", "size-7", "size-8", "size-9", "size-10",
];

const INTERACTIVE_TAGS: &[&str] = &["button", "a", "input"];

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_opening_element" && kind != "jsx_self_closing_element" {
        return;
    }

    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };
    if !INTERACTIVE_TAGS.contains(&tag) {
        return;
    }

    let mut class_value: Option<String> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = crate::rules::jsx::jsx_attribute_name(child, source) else { continue };
        if name != "className" { continue; }
        if let Some(s) = crate::rules::jsx::jsx_attribute_string_value(child, source) {
            class_value = Some(s.to_string());
        }
    }

    let Some(cls) = class_value else { return };
    let tokens: Vec<&str> = cls.split_ascii_whitespace().collect();
    let has_tiny_h = tokens.iter().any(|t| t.starts_with("h-") && TINY_SIZE_TOKENS.contains(t));
    let has_tiny_w = tokens.iter().any(|t| t.starts_with("w-") && TINY_SIZE_TOKENS.contains(t));
    let has_tiny_size = tokens.iter().any(|t| t.starts_with("size-") && TINY_SIZE_TOKENS.contains(t));

    let tiny = (has_tiny_h && has_tiny_w) || has_tiny_size;
    if !tiny { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "<{tag}> has a tap area under 44×44 px (className `{cls}`) — add padding or grow the hit target."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_small_button() {
        let src = r#"const x = <button className="h-4 w-4" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_small_size_anchor() {
        let src = r##"const x = <a className="size-3" href="#">x</a>;"##;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_44px_button() {
        let src = r#"const x = <button className="h-12 w-12" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_interactive() {
        let src = r#"const x = <div className="h-4 w-4" />;"#;
        assert!(run(src).is_empty());
    }
}

//! html-require-button-type AST backend.
//!
//! Walks JSX opening / self-closing elements; whenever the tag is
//! `button`, requires a `type` attribute to be present.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_opening_element" && kind != "jsx_self_closing_element" {
        return;
    }

    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "button" {
        return;
    }

    let mut cursor = node.walk();
    let has_type = node.children(&mut cursor).any(|child| {
        crate::rules::jsx::jsx_attribute_name(child, source) == Some("type")
    });

    if has_type {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "html-require-button-type".into(),
        message: "`<button>` is missing an explicit `type` attribute (defaults to `submit` inside forms).".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_button_without_type() {
        assert_eq!(run(r#"const x = <button>Save</button>;"#).len(), 1);
    }

    #[test]
    fn flags_self_closing_button_without_type() {
        assert_eq!(run(r#"const x = <button />;"#).len(), 1);
    }

    #[test]
    fn allows_button_with_type() {
        assert!(run(r#"const x = <button type="button">Save</button>;"#).is_empty());
    }

    #[test]
    fn allows_button_type_submit() {
        assert!(run(r#"const x = <button type="submit">Go</button>;"#).is_empty());
    }

    #[test]
    fn ignores_non_button() {
        assert!(run(r#"const x = <div>Save</div>;"#).is_empty());
    }
}

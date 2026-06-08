//! a11y-anchor-has-content backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let is_self_closing = node.kind() == "jsx_self_closing_element";
    let is_element = node.kind() == "jsx_element";

    if !is_self_closing && !is_element {
        return;
    }

    // For self-closing, check tag directly; for element, check opening tag.
    let tag_node = if is_self_closing {
        node
    } else {
        let Some(opening) = node.child(0) else { return };
        if opening.kind() != "jsx_opening_element" { return; }
        opening
    };

    let Some(name_node) = tag_node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };
    if tag != "a" { return; }

    // Check for aria-label attribute
    let mut cursor = tag_node.walk();
    let has_aria_label = tag_node.children(&mut cursor).any(|child| {
        if child.kind() != "jsx_attribute" { return false; }
        let Some(attr_name) = child.child(0) else { return false };
        let Ok(name_text) = attr_name.utf8_text(source) else { return false };
        name_text == "aria-label" || name_text == "aria-labelledby"
    });
    if has_aria_label { return; }

    if is_self_closing {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-anchor-has-content".into(),
            message: "Anchor is self-closing and has no content for screen readers.".into(),
            severity: Severity::Error,
            span: None,
        });
        return;
    }

    // For jsx_element, check if there is any non-whitespace text content or child elements
    let mut el_cursor = node.walk();
    let has_content = node.children(&mut el_cursor).any(|child| {
        match child.kind() {
            "jsx_text" => {
                let Ok(text) = child.utf8_text(source) else { return false };
                !text.trim().is_empty()
            }
            "jsx_element" | "jsx_self_closing_element" | "jsx_expression" => true,
            _ => false,
        }
    });

    if !has_content {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-anchor-has-content".into(),
            message: "Anchor has no content — screen readers cannot announce it.".into(),
            severity: Severity::Error,
            span: None,
        });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_self_closing_anchor() {
        assert_eq!(run_on("const x = <a href=\"/home\" />;").len(), 1);
    }

    #[test]
    fn flags_empty_anchor() {
        assert_eq!(run_on("const x = <a href=\"/home\"></a>;").len(), 1);
    }

    #[test]
    fn allows_anchor_with_content() {
        assert!(run_on("const x = <a href=\"/home\">Home</a>;").is_empty());
    }

    #[test]
    fn allows_anchor_with_aria_label() {
        assert!(run_on("const x = <a href=\"/home\" aria-label=\"Home\" />;").is_empty());
    }
}

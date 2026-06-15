//! react-jsx-no-target-blank AST backend.
//!
//! Flags `target="_blank"` on JSX elements whose `rel` does not contain a
//! `noopener` or `noreferrer` token (either severs `window.opener`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_element"] => |node, source, ctx, diagnostics|
    let is_self_closing = node.kind() == "jsx_self_closing_element";

    // For jsx_element, inspect the opening tag; for self-closing, the node itself.
    let tag_node = if is_self_closing {
        node
    } else {
        let Some(opening) = node.child(0) else { return };
        if opening.kind() != "jsx_opening_element" { return; }
        opening
    };

    // Scan attributes for target="_blank" and a rel that severs `window.opener`.
    let mut cursor = tag_node.walk();
    let mut has_target_blank = false;
    let mut has_safe_rel = false;

    for child in tag_node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(name_text) = crate::rules::jsx::jsx_attribute_name(child, source) else { continue };
        let Some(value_text) = crate::rules::jsx::jsx_attribute_string_value(child, source) else {
            continue;
        };

        match name_text {
            "target" => {
                if value_text.contains("_blank") {
                    has_target_blank = true;
                }
            }
            "rel" => {
                if super::rel_is_safe(value_text) {
                    has_safe_rel = true;
                }
            }
            _ => {}
        }
    }

    if has_target_blank && !has_safe_rel {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-jsx-no-target-blank".into(),
            message: "`target=\"_blank\"` without `rel=\"noopener\"` (or `noreferrer`) \
                      allows the opened page to access `window.opener`. \
                      Add `rel=\"noopener\"`."
                .into(),
            severity: Severity::Warning,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_target_blank_without_rel() {
        let src = r#"const x = <a href="https://example.com" target="_blank">link</a>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_target_blank_with_noreferrer() {
        let src =
            r#"const x = <a href="https://example.com" target="_blank" rel="noreferrer">link</a>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_target_blank_with_noopener_noreferrer() {
        let src = r#"const x = <a href="https://example.com" target="_blank" rel="noopener noreferrer">link</a>;"#;
        assert!(run(src).is_empty());
    }

    // `noopener` alone fully severs `window.opener`, so it closes the vector this
    // rule targets. Regression for #3379 (`<RouterLink target="_blank" rel="noopener" />`).
    #[test]
    fn allows_target_blank_with_noopener_only() {
        let src =
            r#"const x = <a href="https://example.com" target="_blank" rel="noopener">link</a>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_target_blank_with_noreferrer_noopener_reversed() {
        let src = r#"const x = <a href="https://example.com" target="_blank" rel="noreferrer noopener">link</a>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_target_blank_with_noopener_among_other_tokens() {
        let src = r#"const x = <a href="https://example.com" target="_blank" rel="nofollow noopener">link</a>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_target_blank_with_unsafe_rel_only() {
        let src =
            r#"const x = <a href="https://example.com" target="_blank" rel="nofollow">link</a>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_no_target_blank() {
        let src = r#"const x = <a href="https://example.com">link</a>;"#;
        assert!(run(src).is_empty());
    }
}

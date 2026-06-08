//! a11y-anchor-ambiguous-text backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

const AMBIGUOUS_TEXTS: &[&str] = &[
    "click here",
    "here",
    "link",
    "a link",
    "read more",
    "learn more",
];

crate::ast_check! { on ["jsx_element"] => |node, source, ctx, diagnostics|
    // We need a full jsx_element (opening + children + closing) to inspect text content.
    // Check the opening tag is an <a>
    let Some(opening) = node.child(0) else { return };
    if opening.kind() != "jsx_opening_element" { return; }
    let Some(tag_name) = opening.child_by_field_name("name") else { return };
    let Ok(tag) = tag_name.utf8_text(source) else { return };
    if tag != "a" { return; }

    // Collect text content from jsx_text children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_text" { continue; }
        let Ok(text) = child.utf8_text(source) else { continue };
        let trimmed = text.trim().to_lowercase();
        for &ambiguous in AMBIGUOUS_TEXTS {
            if trimmed == ambiguous {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "a11y-anchor-ambiguous-text".into(),
                    message: format!(
                        "Ambiguous link text \"{ambiguous}\". Use descriptive text that indicates the link's purpose."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                return; // one diagnostic per element
            }
        }
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
    fn flags_click_here() {
        let d = run_on(r#"const x = <a href="/page">click here</a>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("click here"));
    }

    #[test]
    fn flags_read_more_case_insensitive() {
        let d = run_on(r#"const x = <a href="/page">Read More</a>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_descriptive_text() {
        assert!(run_on(r#"const x = <a href="/docs">View documentation</a>;"#).is_empty());
    }

    #[test]
    fn flags_here() {
        let d = run_on(r#"const x = <a href="/page">here</a>;"#);
        assert_eq!(d.len(), 1);
    }
}

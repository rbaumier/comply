//! react-jsx-no-duplicate-props AST backend.
//!
//! Walks `jsx_opening_element` nodes, collects prop names from their
//! `jsx_attribute` children, and flags duplicates.

use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::FxHashSet;

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let mut seen = FxHashSet::default();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(name_node) = child.child(0) else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(source) else {
            continue;
        };
        if !seen.insert(name.to_string()) {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "react-jsx-no-duplicate-props".into(),
                message: format!(
                    "Duplicate JSX prop `{name}` — the last value silently wins."
                ),
                severity: Severity::Error,
                span: None,
            });
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
    fn flags_duplicate_prop() {
        let src = r#"const x = <div className="a" className="b" />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_duplicates() {
        let src = r#"const x = <input type="text" value="a" type="number" value="b" />;"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_unique_props() {
        let src = r#"const x = <div className="a" id="b" />;"#;
        assert!(run_on(src).is_empty());
    }
}

//! no-nested-ternary — flags ternary expressions nested inside another ternary.
//!
//! Why: nested ternaries are hard to read and easy to misparse visually.
//! Extract to if/else or assign each branch to a named variable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::Rule;
use crate::rules::walker::walk_tree;
use std::path::Path;

pub struct NoNestedTernary;

impl Rule for NoNestedTernary {
    fn id(&self) -> &'static str {
        "no-nested-ternary"
    }

    fn languages(&self) -> &[Language] {
        &[Language::TypeScript, Language::Tsx, Language::JavaScript]
    }

    fn needs_tree(&self) -> bool {
        true
    }

    fn check_tree(
        &self,
        path: &Path,
        _source: &[u8],
        tree: &tree_sitter::Tree,
        _language: Language,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "ternary_expression" {
                return;
            }
            let parent_is_ternary = node
                .parent()
                .is_some_and(|p| p.kind() == "ternary_expression");
            if !parent_is_ternary {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: self.id().into(),
                message: "Nested ternary — extract to if/else or a named variable \
                          for each branch."
                    .into(),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint_ts_with;

    #[test]
    fn flags_nested_ternary() {
        let source = "const x = a ? b ? 1 : 2 : 3;";
        let diags = lint_ts_with(&NoNestedTernary, source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-nested-ternary");
    }

    #[test]
    fn allows_single_ternary() {
        let source = "const x = a ? 1 : 2;";
        let diags = lint_ts_with(&NoNestedTernary, source);
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_deeply_nested_ternaries() {
        let source = "const x = a ? b ? c ? 1 : 2 : 3 : 4;";
        let diags = lint_ts_with(&NoNestedTernary, source);
        assert_eq!(diags.len(), 2);
    }
}

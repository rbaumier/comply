//! no-nested-ternary — flags ternary expressions nested inside another ternary.
//!
//! Why: nested ternaries are hard to read and easy to misparse visually.
//! Extract to if/else or assign each branch to a named variable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::Rule;
use std::path::Path;

pub struct NoNestedTernary;

impl Rule for NoNestedTernary {
    fn id(&self) -> &'static str {
        "no-nested-ternary"
    }

    fn languages(&self) -> &[Language] {
        &[Language::TypeScript]
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
        let mut cursor = tree.walk();
        collect_nested_ternaries(&mut cursor, path, self.id(), &mut diagnostics);
        diagnostics
    }
}

/// Walk the tree and flag any ternary_expression whose parent is also a ternary_expression.
fn collect_nested_ternaries(
    cursor: &mut tree_sitter::TreeCursor,
    path: &Path,
    rule_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    loop {
        let node = cursor.node();

        if node.kind() == "ternary_expression"
            && node
                .parent()
                .is_some_and(|p| p.kind() == "ternary_expression")
        {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: rule_id.into(),
                message: "Nested ternary — extract to if/else or a named variable \
                          for each branch."
                    .into(),
                severity: Severity::Error,
            });
        }

        if cursor.goto_first_child() {
            collect_nested_ternaries(cursor, path, rule_id, diagnostics);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::run_rule_on_ts;

    #[test]
    fn flags_nested_ternary() {
        let source = "const x = a ? b ? 1 : 2 : 3;";
        let diags = run_rule_on_ts(&NoNestedTernary, source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-nested-ternary");
    }

    #[test]
    fn allows_single_ternary() {
        let source = "const x = a ? 1 : 2;";
        let diags = run_rule_on_ts(&NoNestedTernary, source);
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_deeply_nested_ternaries() {
        let source = "const x = a ? b ? c ? 1 : 2 : 3 : 4;";
        let diags = run_rule_on_ts(&NoNestedTernary, source);
        // 2 nested levels (b? and c?).
        assert_eq!(diags.len(), 2);
    }
}

//! no-throw — flags every `throw` statement in TypeScript.
//!
//! Why: thrown exceptions are invisible in function signatures — callers can't
//! know what might explode. Use Result<T, E> to surface errors as values.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::Rule;
use crate::rules::walker::walk_tree;
use std::path::Path;

pub struct NoThrow;

impl Rule for NoThrow {
    fn id(&self) -> &'static str {
        "no-throw"
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
        walk_tree(tree, |node| {
            if node.kind() != "throw_statement" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: self.id().into(),
                message: "Use Result<T, E> instead of throw — surface errors as values, \
                          not exceptions. Callers can't see thrown errors in the type signature."
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
    fn flags_throw_statement() {
        let source = "function f() { throw new Error('boom'); }";
        let diags = lint_ts_with(&NoThrow, source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-throw");
    }

    #[test]
    fn allows_code_without_throw() {
        let source = "function f() { return 42; }";
        let diags = lint_ts_with(&NoThrow, source);
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_multiple_throws() {
        let source = "function f() { throw 1; } function g() { throw 2; }";
        let diags = lint_ts_with(&NoThrow, source);
        assert_eq!(diags.len(), 2);
    }
}

//! rust-no-pub-use-glob backend.
//!
//! Walks `use_declaration` nodes whose source text starts with `pub`
//! and ends with `*;`. We use the textual form rather than the AST
//! because the wildcard is represented as a `use_wildcard` node
//! deep in the tree, and the `pub` modifier is a separate child —
//! easier to scan the line.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "use_declaration" {
                return;
            }
            let Ok(text) = node.utf8_text(source_bytes) else {
                return;
            };
            // Strip leading whitespace, check `pub use … *;` shape.
            let trimmed = text.trim_start();
            if !trimmed.starts_with("pub use") && !trimmed.starts_with("pub(") {
                return;
            }
            // The `pub(crate)` form is OK — we only complain about the
            // truly public `pub use`. Detect by checking for `pub use ` exactly
            // OR `pub use ` after a `pub(scope)` modifier.
            if trimmed.starts_with("pub(crate)") || trimmed.starts_with("pub(super)") {
                return;
            }
            // Must end with the wildcard import.
            if !trimmed.trim_end().trim_end_matches(';').trim_end().ends_with("::*") {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-pub-use-glob".into(),
                message: "`pub use ...::*` re-exports every public symbol \
                          from the source module — your crate's API \
                          quietly mirrors theirs. List the names explicitly: \
                          `pub use foo::{Bar, Baz};`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_pub_use_glob() {
        assert_eq!(run_on("pub use crate::types::*;").len(), 1);
    }

    #[test]
    fn allows_pub_use_explicit_list() {
        assert!(run_on("pub use crate::types::{Foo, Bar};").is_empty());
    }

    #[test]
    fn allows_private_use_glob() {
        assert!(run_on("use crate::types::*;").is_empty());
    }

    #[test]
    fn allows_pub_crate_use_glob() {
        // pub(crate) doesn't escape the crate — internal scope, fine.
        assert!(run_on("pub(crate) use crate::types::*;").is_empty());
    }
}

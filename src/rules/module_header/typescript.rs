//! module-header backend — the first non-whitespace content of every file
//! must be a JSDoc comment describing what the module does and how it works.
//!
//! Why: a reader opening the file should immediately know what it's looking
//! at. A one-line `// foo` label doesn't do that — neither does jumping
//! straight into imports. A real `/** ... */` at the top forces the author
//! to name the module's purpose.
//!
//! Detection: the first `program` child must be a `comment` node starting
//! with `/**`. If the first child is an import, a declaration, or a bare
//! `//` comment, the rule fires.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let root = tree.root_node();
        let source_bytes = ctx.source.as_bytes();

        let Some(first_child) = root.child(0) else {
            return vec![];
        };
        if is_jsdoc_comment(first_child, source_bytes) {
            return vec![];
        }
        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: 1,
            column: 1,
            rule_id: "module-header".into(),
            message: "File is missing a module-header JSDoc block at the top. \
                      Add `/** What this module does. How it works. */` so \
                      readers know what they're looking at immediately."
                .into(),
            severity: Severity::Warning,
        }]
    }
}

fn is_jsdoc_comment(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "comment" {
        return false;
    }
    node.utf8_text(source).is_ok_and(|t| t.starts_with("/**"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn allows_file_starting_with_jsdoc() {
        let source = "/** This module does X. */\nexport const foo = 1;";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_file_starting_with_import() {
        let source = "import { x } from 'y';\nexport const foo = 1;";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_file_starting_with_declaration() {
        assert_eq!(run_on("export const foo = 1;").len(), 1);
    }

    #[test]
    fn flags_file_starting_with_line_comment() {
        let source = "// Some module\nexport const foo = 1;";
        assert_eq!(run_on(source).len(), 1);
    }
}

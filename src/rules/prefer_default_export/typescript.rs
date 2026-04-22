//! prefer-default-export backend — flag modules that have exactly one named
//! export and no default export.
//!
//! Walks the top-level `program` children once. Counts:
//! - each `export_statement` that is NOT `export default …`
//! - each `export_statement` that IS `export default …`
//! - re-exports (`export { x } from …`) count as named exports.
//!
//! If there is exactly one named export and no default export, flag the
//! lone named export.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let root = tree.root_node();

        let mut named_exports: Vec<tree_sitter::Node> = Vec::new();
        let mut has_default = false;

        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.kind() != "export_statement" {
                continue;
            }
            let text = match child.utf8_text(source) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let trimmed = text.trim_start();
            if trimmed.starts_with("export default ") || trimmed.starts_with("export default\n") {
                has_default = true;
            } else {
                named_exports.push(child);
            }
        }

        if has_default || named_exports.len() != 1 {
            return Vec::new();
        }

        let only = named_exports[0];
        let pos = only.start_position();
        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-default-export".into(),
            message: "Prefer `export default` when a module has a single export.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_single_named_export() {
        assert_eq!(run_on("export const foo = 1;").len(), 1);
    }

    #[test]
    fn flags_single_named_function() {
        assert_eq!(run_on("export function foo() {}").len(), 1);
    }

    #[test]
    fn allows_multiple_named_exports() {
        assert!(run_on("export const a = 1;\nexport const b = 2;").is_empty());
    }

    #[test]
    fn allows_default_export() {
        assert!(run_on("export default function foo() {}").is_empty());
    }

    #[test]
    fn allows_named_plus_default() {
        assert!(run_on("export const a = 1;\nexport default 2;").is_empty());
    }

    #[test]
    fn allows_no_exports() {
        assert!(run_on("const x = 1;").is_empty());
    }
}

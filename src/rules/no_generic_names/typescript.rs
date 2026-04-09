//! no-generic-names backend — reject `data`, `info`, `temp`, `result`,
//! `obj`, `item` used as standalone identifiers.
//!
//! Why: a name like `data` or `result` carries zero information about what
//! the variable actually represents. Two weeks later, the author reads
//! their own code and has to trace the flow to remember what `data` meant.
//! Rename to describe what the value IS — `parsedOrder`, `userProfile`,
//! `paymentReceipt`.
//!
//! Note: `data` is also in `banned_identifiers` as a prefix, but that rule
//! fires on compound names like `dataSource`. This rule fires on the
//! standalone `data` identifier.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const GENERIC_NAMES: &[&str] = &[
    "data", "info", "temp", "result", "obj", "item", "thing", "stuff", "val",
    "retval",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "identifier" {
                return;
            }
            // Only flag at DECLARATION sites, not every use. Otherwise the
            // rule would fire a hundred times for one variable.
            if !is_declaration_site(node) {
                return;
            }
            let Ok(name) = node.utf8_text(source_bytes) else {
                return;
            };
            let lower = name.to_ascii_lowercase();
            if !GENERIC_NAMES.contains(&lower.as_str()) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-generic-names".into(),
                message: format!(
                    "Identifier '{name}' carries no meaning — rename to \
                     describe what the value IS (`parsedOrder`, \
                     `userProfile`, `paymentReceipt`)."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// True when the identifier appears directly inside a declaring context
/// (variable_declarator, required_parameter, catch_clause) rather than as
/// a reference to an existing binding.
fn is_declaration_site(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "variable_declarator" | "required_parameter" | "catch_clause"
    )
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
            &CheckCtx {
                path: Path::new("t.ts"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn flags_const_data() {
        assert_eq!(run_on("const data = 5;").len(), 1);
    }

    #[test]
    fn flags_let_temp() {
        assert_eq!(run_on("let temp = 1;").len(), 1);
    }

    #[test]
    fn flags_function_param_result() {
        assert_eq!(run_on("function f(result: number) {}").len(), 1);
    }

    #[test]
    fn allows_descriptive_names() {
        assert!(run_on("const parsedOrder = 1;").is_empty());
        assert!(run_on("const userProfile = {};").is_empty());
    }

    #[test]
    fn does_not_flag_compound_name() {
        // `dataSource` is not a bare `data`; banned_identifiers covers this case.
        assert!(run_on("const dataSource = 1;").is_empty());
    }
}

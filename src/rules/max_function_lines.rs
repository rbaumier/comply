//! max-function-lines — flags functions exceeding 30 lines.
//!
//! Why: long functions mix abstraction levels and resist testing.
//! Extract a named helper at line 30.
//!
//! Uses tree-sitter to find function_declaration, method_definition,
//! and arrow_function nodes, then counts lines (end_row - start_row + 1).

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::Rule;
use std::path::Path;

const MAX_LINES: usize = 30;

/// Node kinds that represent function bodies in TypeScript/TSX.
const TS_FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "method_definition",
    "arrow_function",
];

pub struct MaxFunctionLines;

impl Rule for MaxFunctionLines {
    fn id(&self) -> &'static str {
        "max-function-lines"
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
        source: &[u8],
        tree: &tree_sitter::Tree,
        _language: Language,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut cursor = tree.walk();
        collect_functions(&mut cursor, source, path, self.id(), &mut diagnostics);
        diagnostics
    }
}

/// Recursively walk the tree looking for function nodes and check their line count.
fn collect_functions(
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    path: &Path,
    rule_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    loop {
        let node = cursor.node();

        if TS_FUNCTION_KINDS.contains(&node.kind()) {
            let start = node.start_position();
            let end = node.end_position();
            let line_count = end.row - start.row + 1;

            if line_count > MAX_LINES {
                // Try to extract function name from first named child (identifier).
                let name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("<anonymous>");

                diagnostics.push(Diagnostic {
                    path: path.to_path_buf(),
                    line: start.row + 1, // tree-sitter rows are 0-indexed.
                    column: start.column + 1,
                    rule_id: rule_id.into(),
                    message: format!(
                        "Function '{name}' is {line_count} lines (max {MAX_LINES}). \
                         Extract a named helper for the logic below line {}.",
                        start.row + 1 + MAX_LINES
                    ),
                    severity: Severity::Error,
                });
            }
        }

        // Recurse into children.
        if cursor.goto_first_child() {
            collect_functions(cursor, source, path, rule_id, diagnostics);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

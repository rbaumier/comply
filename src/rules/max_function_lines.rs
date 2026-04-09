//! max-function-lines — flags functions exceeding 30 lines.
//!
//! Why: long functions mix abstraction levels and resist testing.
//! Extract a named helper at line 30.
//!
//! Uses tree-sitter to find function_declaration, method_definition,
//! and arrow_function nodes, then counts lines (end_row - start_row + 1).
//! `saturating_sub` guards against malformed nodes where end_row < start_row
//! (rare, can happen on parse errors).

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::Rule;
use crate::rules::walker::walk_tree;
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
        &[Language::TypeScript, Language::Tsx, Language::JavaScript]
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
        walk_tree(tree, |node| {
            if !TS_FUNCTION_KINDS.contains(&node.kind()) {
                return;
            }
            let start = node.start_position();
            let end = node.end_position();
            // saturating_sub: defensive against malformed nodes where end < start.
            let line_count = end.row.saturating_sub(start.row) + 1;
            if line_count <= MAX_LINES {
                return;
            }
            // Try to extract function name from named child.
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("<anonymous>");

            diagnostics.push(Diagnostic {
                path: path.to_path_buf(),
                line: start.row + 1,
                column: start.column + 1,
                rule_id: self.id().into(),
                message: format!(
                    "Function '{name}' is {line_count} lines (max {MAX_LINES}). \
                     Extract a named helper for the logic below line {}.",
                    start.row + 1 + MAX_LINES
                ),
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
    fn flags_long_function() {
        let body = "let x = 0;\n".repeat(MAX_LINES + 5);
        let source = format!("function long() {{\n{body}}}");
        let diags = lint_ts_with(&MaxFunctionLines, &source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "max-function-lines");
    }

    #[test]
    fn allows_short_function() {
        let source = "function short() { return 42; }";
        let diags = lint_ts_with(&MaxFunctionLines, source);
        assert!(diags.is_empty());
    }

    #[test]
    fn extracts_function_name_in_message() {
        let body = "let x = 0;\n".repeat(MAX_LINES + 1);
        let source = format!("function myLongFunc() {{\n{body}}}");
        let diags = lint_ts_with(&MaxFunctionLines, &source);
        assert!(diags[0].message.contains("myLongFunc"));
    }
}

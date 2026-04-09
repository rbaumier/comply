//! jsdoc-on-exported backend — every exported function must have a
//! preceding JSDoc block.
//!
//! Why: exported functions are the module's public contract. A JSDoc block
//! tells callers what the function does, what it returns, and what can go
//! wrong — without it, every caller has to re-read the implementation.
//!
//! Detection: walk program-level `export_statement` nodes whose child is
//! a `function_declaration`. Check that the export is preceded by a
//! `comment` node starting with `/**` (JSDoc marker). Comments above the
//! export at program level count as "preceding".

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let root = tree.root_node();
        let mut diagnostics = Vec::new();

        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() != "export_statement" {
                continue;
            }
            if !is_exported_function(child) {
                continue;
            }
            if has_jsdoc_predecessor(child, source_bytes) {
                continue;
            }
            let name = extract_exported_name(child, source_bytes).unwrap_or("<anonymous>");
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "jsdoc-on-exported".into(),
                message: format!(
                    "Exported function '{name}' is missing a JSDoc block. \
                     Add a `/** ... */` describing what it does, its params, \
                     and what it returns — this is the public API contract."
                ),
                severity: Severity::Warning,
            });
        }
        diagnostics
    }
}

/// True if the export_statement wraps a function_declaration.
fn is_exported_function(export: tree_sitter::Node) -> bool {
    let mut cursor = export.walk();
    for child in export.children(&mut cursor) {
        if child.kind() == "function_declaration" {
            return true;
        }
    }
    false
}

/// True if the export is preceded (at sibling level) by a `/** */` comment.
fn has_jsdoc_predecessor(export: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(prev) = export.prev_named_sibling() else {
        return false;
    };
    if prev.kind() != "comment" {
        return false;
    }
    let Ok(text) = prev.utf8_text(source) else {
        return false;
    };
    text.starts_with("/**")
}

/// Extract the function name from the exported declaration.
fn extract_exported_name<'a>(export: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = export.walk();
    for child in export.children(&mut cursor) {
        if child.kind() != "function_declaration" {
            continue;
        }
        return child
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok());
    }
    None
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
    fn flags_exported_function_without_jsdoc() {
        assert_eq!(run_on("export function foo() {}").len(), 1);
    }

    #[test]
    fn allows_exported_function_with_jsdoc() {
        let source = "/** Does foo. */\nexport function foo() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_non_exported_function() {
        assert!(run_on("function helper() {}").is_empty());
    }

    #[test]
    fn does_not_flag_single_line_comment() {
        // Single-slash `//` is not JSDoc — flag it.
        let source = "// Does foo.\nexport function foo() {}";
        assert_eq!(run_on(source).len(), 1);
    }
}

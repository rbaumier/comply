//! jsdoc-missing-example backend — every JSDoc on an exported function must
//! contain an `@example` tag.
//!
//! Walks `export_statement` nodes wrapping a `function_declaration`. If the
//! preceding sibling is a JSDoc comment (`/** ... */`) and that comment does
//! NOT contain `@example`, we flag it. Exports without ANY JSDoc are not
//! flagged here — that's `jsdoc-on-exported`'s job. The two rules compose:
//! one ensures presence, the other ensures completeness.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
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
            let Some(jsdoc_text) = jsdoc_text_above(child, source) else {
                // No JSDoc at all — that's jsdoc-on-exported's responsibility,
                // not ours. We only fire when a doc exists but lacks @example.
                continue;
            };
            if jsdoc_text.contains("@example") {
                continue;
            }
            let name = extract_exported_name(child, source).unwrap_or("<anonymous>");
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "jsdoc-missing-example".into(),
                message: format!(
                    "JSDoc on `{name}` is missing `@example`. Add a real call \
                     and its return value — examples are the fastest way for \
                     callers to understand the API."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn is_exported_function(export: tree_sitter::Node) -> bool {
    let mut cursor = export.walk();
    for child in export.children(&mut cursor) {
        if child.kind() == "function_declaration" {
            return true;
        }
    }
    false
}

fn jsdoc_text_above<'a>(export: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let prev = export.prev_named_sibling()?;
    if prev.kind() != "comment" {
        return None;
    }
    let text = prev.utf8_text(source).ok()?;
    if !text.starts_with("/**") {
        return None;
    }
    Some(text)
}

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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_jsdoc_without_example() {
        let source = "/** Does foo. */\nexport function foo() {}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_jsdoc_with_example() {
        let source = "/** Does foo.\n * @example\n *   foo();\n */\nexport function foo() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_export_without_jsdoc() {
        // No JSDoc at all — jsdoc-on-exported's job, not ours.
        assert!(run_on("export function foo() {}").is_empty());
    }

    #[test]
    fn ignores_non_exported_function() {
        let source = "/** Helper. */\nfunction helper() {}";
        assert!(run_on(source).is_empty());
    }
}

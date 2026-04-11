//! jsdoc-require-returns backend — functions with return values must have
//! `@returns` in their JSDoc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

/// Check if a JSDoc comment has a `@returns` or `@return` tag.
fn has_returns_tag(comment_text: &str) -> bool {
    for line in comment_text.lines() {
        let content = line.trim().trim_start_matches('*').trim();
        if content.starts_with("@returns")
            || content.starts_with("@return ")
            || content == "@return"
        {
            return true;
        }
    }
    false
}

/// Check if a function body contains a `return <value>` statement (not bare `return;`).
fn has_return_value(fn_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = fn_node.child_by_field_name("body") else {
        // Arrow functions without braces always return a value.
        return false;
    };

    // Walk the body looking for return_statement nodes.
    let _cursor = body.walk();
    let mut found = false;

    fn search_returns(node: tree_sitter::Node, _source: &[u8], found: &mut bool) {
        if *found {
            return;
        }
        // Don't descend into nested functions.
        match node.kind() {
            "function_declaration"
            | "function"
            | "arrow_function"
            | "generator_function_declaration"
            | "generator_function"
            | "method_definition" => return,
            _ => {}
        }

        if node.kind() == "return_statement" {
            // Check if it has a value child (not bare `return;`).
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "return" && child.kind() != ";" {
                    *found = true;
                    return;
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            search_returns(child, _source, found);
        }
    }

    search_returns(body, source, &mut found);
    found
}

/// Find the function node immediately following a comment node (sibling).
fn find_following_function(comment: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut next = comment.next_named_sibling();
    for _ in 0..3 {
        let sibling = next?;
        match sibling.kind() {
            "function_declaration" | "generator_function_declaration" | "method_definition" => {
                return Some(sibling);
            }
            "export_statement" => {
                let mut cursor = sibling.walk();
                for child in sibling.children(&mut cursor) {
                    if child.kind() == "function_declaration"
                        || child.kind() == "generator_function_declaration"
                    {
                        return Some(child);
                    }
                }
                return None;
            }
            "decorator" => {
                next = sibling.next_named_sibling();
                continue;
            }
            _ => return None,
        }
    }
    None
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if node.kind() != "comment" {
                return;
            }
            let Ok(text) = node.utf8_text(source_bytes) else { return };
            if !text.starts_with("/**") {
                return;
            }

            if has_returns_tag(text) {
                return;
            }

            let Some(fn_node) = find_following_function(node) else { return };

            if has_return_value(fn_node, source_bytes) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "jsdoc-require-returns".into(),
                    message: "Function returns a value but JSDoc is missing `@returns`. \
                              Document what the function returns."
                        .into(),
                    severity: Severity::Warning,
                });
            }
        });

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_missing_returns_on_returning_fn() {
        let source = r#"
/**
 * Adds two numbers.
 * @param a - first
 * @param b - second
 */
function add(a: number, b: number) { return a + b; }
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("@returns"));
    }

    #[test]
    fn allows_void_function_without_returns() {
        let source = r#"
/**
 * Logs a message.
 * @param msg - the message
 */
function log(msg: string) { console.log(msg); }
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_documented_returns() {
        let source = r#"
/**
 * Doubles.
 * @param x - input
 * @returns the doubled value
 */
function double(x: number) { return x * 2; }
"#;
        assert!(run_on(source).is_empty());
    }
}

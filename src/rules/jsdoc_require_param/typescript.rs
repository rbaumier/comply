//! jsdoc-require-param backend — every JSDoc block must document all
//! function parameters with `@param`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

/// Extract parameter names from a function node's formal_parameters.
fn extract_param_names(fn_node: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let Some(params) = fn_node.child_by_field_name("parameters") else {
        return Vec::new();
    };

    let mut names = Vec::new();
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if let Some(name) = param_name_from_node(child, source) {
            names.push(name);
        }
    }
    names
}

/// Extract a parameter name from a parameter node (handles patterns, rest, etc.).
fn param_name_from_node(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "required_parameter" | "optional_parameter" => {
            // The `pattern` field holds the identifier or destructuring.
            let pattern = node.child_by_field_name("pattern")?;
            if pattern.kind() == "identifier" {
                return pattern.utf8_text(source).ok().map(|s| s.to_string());
            }
            // For destructured params, just get the whole text isn't useful;
            // skip them as JSDoc usually documents the object, not destructured names.
            None
        }
        "identifier" => node.utf8_text(source).ok().map(|s| s.to_string()),
        "rest_pattern" => {
            // `...args` — extract the inner identifier.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    return child.utf8_text(source).ok().map(|s| s.to_string());
                }
            }
            None
        }
        _ => None,
    }
}

/// Check if a JSDoc comment text has an `@param <name>` tag.
fn has_param_tag(comment_text: &str, name: &str) -> bool {
    for line in comment_text.lines() {
        let content = line.trim().trim_start_matches('*').trim();
        if let Some(after) = content.strip_prefix("@param") {
            let after = after.trim_start();
            // Skip optional type: `{Type}`
            let after = if let Some(rest) = after.strip_prefix('{') {
                match rest.find('}') {
                    Some(i) => rest[i + 1..].trim_start(),
                    None => after,
                }
            } else {
                after
            };
            let param_name: String = after
                .chars()
                .take_while(|&c| c.is_alphanumeric() || c == '_' || c == '$')
                .collect();
            if param_name == name {
                return true;
            }
        }
    }
    false
}

/// Find the function node immediately following a comment node (sibling).
fn find_following_function(comment: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut next = comment.next_named_sibling();
    // Skip at most a couple of nodes (e.g. decorators, export_statement wrapping a fn).
    for _ in 0..3 {
        let sibling = next?;
        match sibling.kind() {
            "function_declaration" | "generator_function_declaration" | "method_definition" => {
                return Some(sibling);
            }
            "export_statement" => {
                // Check if the export wraps a function.
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

            let Some(fn_node) = find_following_function(node) else { return };
            let actual_params = extract_param_names(fn_node, source_bytes);

            for param in &actual_params {
                if !has_param_tag(text, param) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "jsdoc-require-param".into(),
                        message: format!(
                            "JSDoc is missing `@param {param}`. Document every \
                             parameter so callers understand the API."
                        ),
                        severity: Severity::Warning,
                    });
                }
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
    fn flags_missing_param_doc() {
        let source = r#"
/**
 * Greets a user.
 */
function greet(name: string) {}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("name"));
    }

    #[test]
    fn allows_fully_documented_params() {
        let source = r#"
/**
 * Adds two numbers.
 * @param a - first
 * @param b - second
 */
function add(a: number, b: number) { return a + b; }
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_partially_documented() {
        let source = r#"
/**
 * Process.
 * @param a - first
 */
function process(a: number, b: number) {}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("b"));
    }
}

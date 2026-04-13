//! jsdoc-check-param-names backend — JSDoc `@param` names must match function params.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

/// Extract `@param` names from a JSDoc comment text.
fn extract_jsdoc_param_names(text: &str) -> Vec<(String, usize)> {
    let mut params = Vec::new();
    for (line_offset, line) in text.lines().enumerate() {
        let trimmed = line.trim().trim_start_matches('*').trim();
        if let Some(after_param) = trimmed.strip_prefix("@param") {
            let after_param = after_param.trim_start();
            // Skip optional type annotation `{type}`.
            let name_str = if let Some(rest) = after_param.strip_prefix('{') {
                match rest.find('}') {
                    Some(close) => rest[close + 1..].trim_start(),
                    None => after_param,
                }
            } else {
                after_param
            };
            let param_name: String = name_str
                .chars()
                .take_while(|&c| c.is_alphanumeric() || c == '_' || c == '$')
                .collect();
            if !param_name.is_empty() {
                params.push((param_name, line_offset));
            }
        }
    }
    params
}

/// Extract parameter names from a function node.
fn extract_function_params(func: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let Some(params) = func.child_by_field_name("parameters") else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        match child.kind() {
            "required_parameter" | "optional_parameter" => {
                // The pattern (first child that is an identifier).
                if let Some(pattern) = child.child_by_field_name("pattern") {
                    if pattern.kind() == "identifier"
                        && let Ok(name) = pattern.utf8_text(source) {
                            result.push(name.to_string());
                        }
                    // Destructuring or rest — skip.
                } else {
                    // Try first named child as identifier.
                    let mut inner = child.walk();
                    for c in child.children(&mut inner) {
                        if c.kind() == "identifier" {
                            if let Ok(name) = c.utf8_text(source) {
                                result.push(name.to_string());
                            }
                            break;
                        }
                    }
                }
            }
            "rest_pattern" => {
                let mut inner = child.walk();
                for c in child.children(&mut inner) {
                    if c.kind() == "identifier" {
                        if let Ok(name) = c.utf8_text(source) {
                            result.push(name.to_string());
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    result
}

/// Check if a node is a function-like declaration.
fn is_function_like(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "function_declaration"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "generator_function_declaration"
            | "generator_function"
    )
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if node.kind() != "comment" {
                return;
            }
            let text = match node.utf8_text(source_bytes) {
                Ok(t) => t,
                Err(_) => return,
            };
            if !text.starts_with("/**") {
                return;
            }

            let jsdoc_params = extract_jsdoc_param_names(text);
            if jsdoc_params.is_empty() {
                return;
            }

            // Find the next sibling that is a function-like node.
            let mut next = node.next_named_sibling();
            let func = loop {
                match next {
                    None => return,
                    Some(n) => {
                        if is_function_like(n) {
                            break n;
                        }
                        // Look for export_statement wrapping a function.
                        if n.kind() == "export_statement" {
                            let mut cursor = n.walk();
                            let inner_func = n.children(&mut cursor).find(|c| is_function_like(*c));
                            if let Some(f) = inner_func {
                                break f;
                            }
                        }
                        // Look for lexical_declaration (const f = () => {}).
                        if n.kind() == "lexical_declaration" || n.kind() == "variable_declaration" {
                            let mut found_func = None;
                            let mut stack = vec![n];
                            while let Some(s) = stack.pop() {
                                if is_function_like(s) {
                                    found_func = Some(s);
                                    break;
                                }
                                let mut cursor = s.walk();
                                for child in s.children(&mut cursor) {
                                    stack.push(child);
                                }
                            }
                            if let Some(f) = found_func {
                                break f;
                            }
                        }
                        next = n.next_named_sibling();
                    }
                }
            };

            let actual_params = extract_function_params(func, source_bytes);

            let comment_start_row = node.start_position().row;
            for (jsdoc_name, line_offset) in &jsdoc_params {
                if !actual_params.iter().any(|p| p == jsdoc_name) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: comment_start_row + line_offset + 1,
                        column: 1,
                        rule_id: "jsdoc-check-param-names".into(),
                        message: format!(
                            "`@param {jsdoc_name}` does not match any function parameter. Actual params: [{}].",
                            actual_params.join(", ")
                        ),
                        severity: Severity::Warning,
                        span: None,
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
    fn flags_mismatched_param_name() {
        let source = r#"
/**
 * Greets a user.
 * @param nme - the name
 */
function greet(name: string) {}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("nme"));
    }

    #[test]
    fn allows_matching_params() {
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
    fn flags_stale_param() {
        let source = r#"
/**
 * Process data.
 * @param input - data
 * @param options - config
 */
function process(input: string) {}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("options"));
    }
}

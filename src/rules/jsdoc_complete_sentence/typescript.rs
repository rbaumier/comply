//! jsdoc-complete-sentence backend — JSDoc descriptions must start with a
//! capital letter and end with punctuation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// Extract description lines from a JSDoc comment text (excluding @tag lines).
fn extract_description_lines(text: &str) -> Vec<(String, usize)> {
    let mut description_lines = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let content = trimmed
            .trim_start_matches("/**")
            .trim_start_matches("*/")
            .trim_start_matches('*')
            .trim_end_matches("*/")
            .trim();

        if content.is_empty() || content == "/" || content.starts_with('@') {
            continue;
        }

        description_lines.push((content.to_string(), i));
    }

    description_lines
}

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["comment"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let text = match node.utf8_text(source_bytes) {
            Ok(t) => t,
            Err(_) => return,
        };
        if !text.starts_with("/**") {
            return;
        }

        let description_lines = extract_description_lines(text);
        if description_lines.is_empty() {
            return;
        }

        let comment_start_row = node.start_position().row;

        // First line must start with a capital letter.
        let (first_text, first_offset) = &description_lines[0];
        if let Some(ch) = first_text.chars().next()
            && ch.is_alphabetic() && !ch.is_uppercase() {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: comment_start_row + first_offset + 1,
                    column: 1,
                    rule_id: "jsdoc-complete-sentence".into(),
                    message: "JSDoc description must start with a capital letter.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }

        // Last line must end with punctuation.
        let (last_text, last_offset) = &description_lines[description_lines.len() - 1];
        if let Some(ch) = last_text.trim_end().chars().last()
            && ch != '.' && ch != '!' && ch != '?' {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: comment_start_row + last_offset + 1,
                    column: 1,
                    rule_id: "jsdoc-complete-sentence".into(),
                    message: "JSDoc description must end with `.`, `!`, or `?`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_lowercase_start() {
        let source = r#"
/**
 * adds two numbers.
 */
function add(a: number, b: number) {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("capital")));
    }

    #[test]
    fn flags_missing_punctuation() {
        let source = r#"
/**
 * Adds two numbers
 */
function add(a: number, b: number) {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn allows_proper_sentence() {
        let source = r#"
/**
 * Adds two numbers.
 */
function add(a: number, b: number) {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_exclamation() {
        let source = "/** Do not call this directly! */\nfunction internal() {}";
        assert!(run_on(source).is_empty());
    }
}

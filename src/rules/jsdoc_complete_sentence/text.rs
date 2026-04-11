use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let trimmed = lines[i].trim();
            if !trimmed.starts_with("/**") {
                i += 1;
                continue;
            }

            let mut description_lines: Vec<(String, usize)> = Vec::new();
            let mut block_ended = false;

            // Single-line JSDoc.
            if trimmed.contains("*/") && trimmed != "/**" {
                let content = trimmed
                    .trim_start_matches("/**")
                    .trim_end_matches("*/")
                    .trim();
                if !content.is_empty() && !content.starts_with('@') {
                    description_lines.push((content.to_string(), i));
                }
                block_ended = true;
            }

            if !block_ended {
                i += 1;
                while i < lines.len() {
                    let line = lines[i].trim();
                    let is_end = line.contains("*/");

                    let content = line
                        .trim_start_matches("*/")
                        .trim_start_matches('*')
                        .trim();

                    if !content.is_empty()
                        && !content.starts_with('@')
                        && !content.starts_with("*/")
                        && content != "/"
                    {
                        description_lines.push((content.to_string(), i));
                    }

                    if is_end {
                        block_ended = true;
                        break;
                    }
                    i += 1;
                }
            }

            // Validate: first line starts with capital, last line ends with punctuation.
            if block_ended && !description_lines.is_empty() {
                let (first_text, first_line_idx) = &description_lines[0];
                if let Some(ch) = first_text.chars().next()
                    && ch.is_alphabetic()
                    && !ch.is_uppercase()
                {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: first_line_idx + 1,
                        column: 1,
                        rule_id: "jsdoc-complete-sentence".into(),
                        message: "JSDoc description must start with a capital letter.".into(),
                        severity: Severity::Warning,
                    });
                }

                let (last_text, last_line_idx) = &description_lines[description_lines.len() - 1];
                if let Some(ch) = last_text.trim_end().chars().last()
                    && ch != '.'
                    && ch != '!'
                    && ch != '?'
                {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: last_line_idx + 1,
                        column: 1,
                        rule_id: "jsdoc-complete-sentence".into(),
                        message: "JSDoc description must end with `.`, `!`, or `?`.".into(),
                        severity: Severity::Warning,
                    });
                }
            }

            i += 1;
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_lowercase_start() {
        let source = r#"
/**
 * adds two numbers.
 */
function add(a: number, b: number) {}
"#;
        let d = run(source);
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
        let d = run(source);
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
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_exclamation() {
        let source = "/** Do not call this directly! */\nfunction internal() {}";
        assert!(run(source).is_empty());
    }
}

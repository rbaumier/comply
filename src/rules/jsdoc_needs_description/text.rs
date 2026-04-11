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

            // Detect opening of a JSDoc block: `/**`
            if !trimmed.starts_with("/**") {
                i += 1;
                continue;
            }

            let block_start = i;
            let mut has_tag = false;
            let mut has_description = false;
            let mut block_ended = false;

            // Handle single-line JSDoc: `/** @param x */`
            if trimmed.contains("*/") {
                let content = trimmed
                    .trim_start_matches("/**")
                    .trim_end_matches("*/")
                    .trim();
                if !content.is_empty() {
                    if content.starts_with('@') {
                        has_tag = true;
                    } else {
                        has_description = true;
                    }
                }
                block_ended = true;
            }

            if !block_ended {
                i += 1;
                // Scan through the JSDoc block
                while i < lines.len() {
                    let line = lines[i].trim();

                    // Check if block ends
                    let is_end = line.contains("*/");

                    // Strip leading `*` or `*/`
                    let content = line
                        .trim_start_matches("*/")
                        .trim_start_matches('*')
                        .trim();

                    if !content.is_empty() && !content.starts_with("*/") {
                        if content.starts_with('@') {
                            has_tag = true;
                        } else if content != "/" {
                            has_description = true;
                        }
                    }

                    if is_end {
                        block_ended = true;
                        break;
                    }
                    i += 1;
                }
            }

            // Flag: has tags but no prose description
            if block_ended && has_tag && !has_description {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: block_start + 1,
                    column: 1,
                    rule_id: "jsdoc-needs-description".into(),
                    message: "JSDoc block contains only tags — add a prose description explaining what this does and why.".into(),
                    severity: Severity::Warning,
                });
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
    fn flags_tags_only_jsdoc() {
        let source = r#"
/**
 * @param x - the input
 * @returns the output
 */
function foo(x: number): number { return x; }
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("only tags"));
    }

    #[test]
    fn flags_single_line_tag_only() {
        let source = "/** @deprecated */\nfunction old() {}";
        let d = run(source);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_jsdoc_with_description() {
        let source = r#"
/**
 * Computes the square of a number.
 * @param x - the input
 * @returns the squared value
 */
function square(x: number): number { return x * x; }
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_jsdoc_with_description_only() {
        let source = r#"
/**
 * This function does something important.
 */
function important() {}
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_empty_jsdoc() {
        // An empty JSDoc (no tags, no description) is not flagged by this rule
        let source = r#"
/**
 */
function foo() {}
"#;
        assert!(run(source).is_empty());
    }
}

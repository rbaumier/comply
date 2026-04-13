//! jsdoc-require-tags backend — JSDoc must include essential tags.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract JSDoc comment blocks from source. Returns (start_line_0based, text).
fn extract_jsdoc_blocks(source: &str) -> Vec<(usize, &str)> {
    let mut blocks = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
            let start = i;
            let start_line = source[..start].matches('\n').count();
            // find closing */
            if let Some(end_rel) = source[i + 3..].find("*/") {
                let end = i + 3 + end_rel + 2;
                blocks.push((start_line, &source[start..end]));
                i = end;
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }
    blocks
}

/// Extract tag entries from a JSDoc block. Returns (line_offset, tag_name, rest_of_line).
fn extract_tags(block: &str) -> Vec<(usize, String, String)> {
    let mut tags = Vec::new();
    for (line_offset, line) in block.lines().enumerate() {
        let trimmed = line.trim().trim_start_matches('*').trim();
        if let Some(rest) = trimmed.strip_prefix('@') {
            let tag_name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-')
                .collect();
            if !tag_name.is_empty() {
                let after = rest[tag_name.len()..].trim().to_string();
                tags.push((line_offset, tag_name, after));
            }
        }
    }
    tags
}

/// Minimal required tags for function documentation.
const RECOMMENDED_TAGS: &[&str] = &["param", "returns", "return"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let blocks = extract_jsdoc_blocks(ctx.source);
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (block_start, block) in &blocks {
            let tags = extract_tags(block);
            let tag_names: Vec<&str> = tags.iter().map(|(_, t, _)| t.as_str()).collect();

            // Check if this JSDoc precedes a function
            let block_end_line = block_start + block.lines().count();
            let next_code: String = lines
                .iter()
                .skip(block_end_line)
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");

            let is_function = next_code.contains("function ")
                || next_code.contains("=>")
                || next_code.contains("async ");

            if is_function {
                // Check for @param or @returns
                let has_any = RECOMMENDED_TAGS.iter().any(|t| tag_names.contains(t));
                if !has_any && !tag_names.is_empty() {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: block_start + 1,
                        column: 1,
                        rule_id: "jsdoc-require-tags".into(),
                        message: "JSDoc comment on a function should include `@param` or `@returns` tags.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::rules::backend::CheckCtx;

    fn run(source: &str) -> Vec<Diagnostic> {
        let ctx = CheckCtx::for_test(Path::new("t.ts"), source);
        Check.check(&ctx)
    }

    #[test]
    fn flags_function_jsdoc_missing_tags() {
        let src = r#"
/**
 * @deprecated
 */
function foo(name: string) {}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_function_with_param() {
        let src = r#"
/**
 * @param {string} name
 */
function foo(name: string) {}
"#;
        assert!(run(src).is_empty());
    }
}

//! jsdoc-check-template-names backend — `@template` names should be used.

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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (block_start, block) in extract_jsdoc_blocks(ctx.source) {
            let tags = extract_tags(block);
            let template_names: Vec<(usize, String)> = tags
                .iter()
                .filter(|(_, t, _)| t == "template")
                .map(|(lo, _, rest)| {
                    let name: String = rest
                        .trim()
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    (*lo, name)
                })
                .filter(|(_, n)| !n.is_empty())
                .collect();

            if template_names.is_empty() {
                continue;
            }

            // Look for type usage in @param/@returns/@type tags
            let type_text: String = tags
                .iter()
                .filter(|(_, t, _)| t != "template")
                .map(|(_, _, rest)| rest.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            for (lo, name) in &template_names {
                if !type_text.contains(name.as_str()) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: block_start + lo + 1,
                        column: 1,
                        rule_id: "jsdoc-check-template-names".into(),
                        message: format!("`@template {name}` is declared but not referenced in any type expression.",),
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
    fn flags_unused_template() {
        let src = r#"
/**
 * @template T
 * @param {string} name
 */
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("T"));
    }

    #[test]
    fn allows_used_template() {
        let src = r#"
/**
 * @template T
 * @param {T} value
 * @returns {T}
 */
"#;
        assert!(run(src).is_empty());
    }
}

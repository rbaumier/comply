//! jsdoc-check-property-names backend — no duplicate `@property` names.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashSet;

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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (block_start, block) in extract_jsdoc_blocks(ctx.source) {
            let mut seen = HashSet::new();
            for (line_offset, line) in block.lines().enumerate() {
                let trimmed = line.trim().trim_start_matches('*').trim();
                if let Some(after) = trimmed.strip_prefix("@property") {
                    let after = after.trim_start();
                    // Skip optional type annotation `{type}`.
                    let name_str = if let Some(rest) = after.strip_prefix('{') {
                        match rest.find('}') {
                            Some(close) => rest[close + 1..].trim_start(),
                            None => after,
                        }
                    } else {
                        after
                    };
                    let name: String = name_str
                        .chars()
                        .take_while(|&c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.')
                        .collect();
                    if !name.is_empty() && !seen.insert(name.clone()) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: block_start + line_offset + 1,
                            column: 1,
                            rule_id: "jsdoc-check-property-names".into(),
                            message: format!("Duplicate `@property` name `{name}`.",),
                            severity: Severity::Warning,
                        });
                    }
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
    fn flags_duplicate_property() {
        let src = r#"
/**
 * @typedef Foo
 * @property {string} name - the name
 * @property {string} name - duplicate
 */
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("name"));
    }

    #[test]
    fn allows_unique_properties() {
        let src = r#"
/**
 * @typedef Foo
 * @property {string} name
 * @property {number} age
 */
"#;
        assert!(run(src).is_empty());
    }
}

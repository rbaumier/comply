//! jsdoc-check-tag-names backend — only recognized JSDoc tag names.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const KNOWN_TAGS: &[&str] = &[
    "abstract", "access", "alias", "arg", "argument", "async", "augments",
    "author", "borrows", "callback", "class", "classdesc", "constant",
    "const", "constructs", "copyright", "default", "defaultvalue",
    "deprecated", "description", "desc", "emits", "enum", "event",
    "example", "exception", "exports", "extends", "external", "file",
    "fileoverview", "fires", "function", "func", "generator", "global",
    "hideconstructor", "host", "ignore", "implements", "import",
    "inheritdoc", "inner", "instance", "interface", "kind", "lends",
    "license", "link", "linkcode", "linkplain", "listens", "member",
    "memberof", "method", "mixes", "mixin", "module", "name", "namespace",
    "next", "override", "overload", "package", "param", "private", "prop",
    "property", "protected", "public", "readonly", "rejects", "requires",
    "returns", "return", "satisfies", "see", "since", "static", "summary",
    "template", "this", "throws", "todo", "tutorial", "type", "typedef",
    "var", "variation", "version", "virtual", "yields", "yield",
    // TypeScript-specific
    "import", "jsx", "jsxFrag", "jsxImportSource", "jsxRuntime",
];

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
            for (line_offset, tag_name, _) in extract_tags(block) {
                if !KNOWN_TAGS.contains(&tag_name.as_str()) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: block_start + line_offset + 1,
                        column: 1,
                        rule_id: "jsdoc-check-tag-names".into(),
                        message: format!("Unknown JSDoc tag `@{tag_name}`.",),
                        severity: Severity::Warning,
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
    fn flags_unknown_tag() {
        let src = r#"
/**
 * @foobar something
 */
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("foobar"));
    }

    #[test]
    fn allows_known_tags() {
        let src = r#"
/**
 * @param name - the name
 * @returns the result
 */
"#;
        assert!(run(src).is_empty());
    }
}

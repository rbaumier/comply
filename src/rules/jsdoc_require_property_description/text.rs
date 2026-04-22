//! jsdoc/require-property-description — every `@property` tag needs a description.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, parse_tags, property_tag_has_description};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for block in find_jsdoc_blocks(ctx.source) {
            for tag in parse_tags(&block.content) {
                if tag.name != "property" {
                    continue;
                }
                if !property_tag_has_description(&tag.value) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: block.start_line + tag.line_offset + 1,
                        column: 1,
                        rule_id: "jsdoc/require-property-description".into(),
                        message: "@property tag is missing a description — document what the property represents.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_property_without_description() {
        let src = "/**\n * @property {string} name\n */\ntype T = { name: string };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_property_with_description() {
        let src = "/**\n * @property {string} name - the user's full name\n */\ntype T = { name: string };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_with_inline_description() {
        let src = "/**\n * @property {string} name description here\n */\ntype T = { name: string };";
        assert!(run(src).is_empty());
    }
}

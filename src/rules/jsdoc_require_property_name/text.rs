//! jsdoc/require-property-name — every `@property` tag needs a name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, parse_tags, strip_type_annotation};

#[derive(Debug)]
pub struct Check;

fn property_tag_has_name(value: &str) -> bool {
    let rest = strip_type_annotation(value.trim());
    let first = rest.split_whitespace().next().unwrap_or("");
    // Name must be a real identifier — reject empty, leading dash (description
    // without a name), or bracketed-only optional markers with no identifier.
    !first.is_empty() && !first.starts_with('-')
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for block in find_jsdoc_blocks(ctx.source) {
            for tag in parse_tags(&block.content) {
                if tag.name != "property" {
                    continue;
                }
                if !property_tag_has_name(&tag.value) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: block.start_line + tag.line_offset + 1,
                        column: 1,
                        rule_id: "jsdoc/require-property-name".into(),
                        message: "@property tag is missing a name — add an identifier after the type.".into(),
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
    fn flags_property_without_name() {
        let src = "/**\n * @property {string}\n */\ntype T = {};";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_property_with_only_description() {
        let src = "/**\n * @property - some description\n */\ntype T = {};";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_property_with_name() {
        let src = "/**\n * @property {string} name\n */\ntype T = { name: string };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_with_name_and_description() {
        let src = "/**\n * @property {string} name - the user's name\n */\ntype T = { name: string };";
        assert!(run(src).is_empty());
    }
}

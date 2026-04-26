//! jsdoc/require-property-description — every `@property` tag needs a description.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_text_helpers::{
    find_jsdoc_blocks, parse_tags, property_tag_has_description,
};

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in find_jsdoc_blocks(text) {
        for tag in parse_tags(&block.content) {
            if tag.name != "property" {
                continue;
            }
            if !property_tag_has_description(&tag.value) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: block.start_line + tag.line_offset + 1 + line_offset,
                    column: 1,
                    rule_id: "jsdoc/require-property-description".into(),
                    message: "@property tag is missing a description — document what the property represents.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
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
        let src =
            "/**\n * @property {string} name description here\n */\ntype T = { name: string };";
        assert!(run(src).is_empty());
    }
}

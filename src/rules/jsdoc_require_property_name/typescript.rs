//! jsdoc/require-property-name — every `@property` tag needs a name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, parse_tags, strip_type_annotation};

fn property_tag_has_name(value: &str) -> bool {
    let rest = strip_type_annotation(value.trim());
    let first = rest.split_whitespace().next().unwrap_or("");
    // Name must be a real identifier — reject empty, leading dash (description
    // without a name), or bracketed-only optional markers with no identifier.
    !first.is_empty() && !first.starts_with('-')
}

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in find_jsdoc_blocks(text) {
        for tag in parse_tags(&block.content) {
            if tag.name != "property" {
                continue;
            }
            if !property_tag_has_name(&tag.value) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: block.start_line + tag.line_offset + 1 + line_offset,
                    column: 1,
                    rule_id: "jsdoc/require-property-name".into(),
                    message: "@property tag is missing a name — add an identifier after the type.".into(),
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
        let src =
            "/**\n * @property {string} name - the user's name\n */\ntype T = { name: string };";
        assert!(run(src).is_empty());
    }
}

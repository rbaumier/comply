//! jsdoc/require-next-description — every `@next` tag needs a description.
//!
//! `@next` is used to document the value passed to the next `.next()` call of
//! a generator. Without a description, readers don't know what to pass in.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_text_helpers::{
    find_jsdoc_blocks, parse_tags, strip_type_annotation, value_has_description,
};

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in find_jsdoc_blocks(text) {
        for tag in parse_tags(&block.content) {
            if tag.name != "next" {
                continue;
            }
            let after_type = strip_type_annotation(&tag.value);
            if !value_has_description(after_type) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: block.start_line + tag.line_offset + 1 + line_offset,
                    column: 1,
                    rule_id: "jsdoc/require-next-description".into(),
                    message: "@next tag is missing a description — explain what the caller passes to .next().".into(),
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
    fn flags_next_without_description() {
        let src = "/**\n * @next {string}\n */\nfunction* g() { yield 1; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bare_next() {
        let src = "/**\n * @next\n */\nfunction* g() { yield 1; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_next_with_description() {
        let src =
            "/**\n * @next {string} the string passed into next()\n */\nfunction* g() { yield 1; }";
        assert!(run(src).is_empty());
    }
}

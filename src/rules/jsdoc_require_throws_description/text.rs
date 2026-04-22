//! jsdoc/require-throws-description — every `@throws` tag needs a description.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::jsdoc_text_helpers::{
    find_jsdoc_blocks, parse_tags, strip_type_annotation, value_has_description,
};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for block in find_jsdoc_blocks(ctx.source) {
            for tag in parse_tags(&block.content) {
                if tag.name != "throws" && tag.name != "exception" {
                    continue;
                }
                let after_type = strip_type_annotation(&tag.value);
                if !value_has_description(after_type) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: block.start_line + tag.line_offset + 1,
                        column: 1,
                        rule_id: "jsdoc/require-throws-description".into(),
                        message: "@throws tag is missing a description — document the failure case.".into(),
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
    fn flags_throws_without_description() {
        let src = "/**\n * @throws {Error}\n */\nfunction f() { throw new Error('x'); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bare_throws() {
        let src = "/**\n * @throws\n */\nfunction f() { throw new Error('x'); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_throws_with_description() {
        let src = "/**\n * @throws {Error} when input is bad\n */\nfunction f() { throw new Error('x'); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_exception_alias_without_description() {
        let src = "/**\n * @exception {Error}\n */\nfunction f() { throw new Error('x'); }";
        assert_eq!(run(src).len(), 1);
    }
}

//! jsdoc/require-template-description — every `@template` tag needs a description.
//!
//! Syntax: `@template [{Constraint}] Name[, Name2] [- description]`. The rule
//! flags `@template T` (no description) but allows `@template T - element type`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, parse_tags, strip_type_annotation};

#[derive(Debug)]
pub struct Check;

fn template_has_description(value: &str) -> bool {
    // Drop optional `{Constraint}` prefix.
    let rest = strip_type_annotation(value.trim());
    // Find the description separator. Template syntax is
    // `Name[, Name2, ...] - description` or `Name description`.
    // Any text past the identifier list counts.
    if let Some(idx) = rest.find(" - ") {
        return !rest[idx + 3..].trim().is_empty();
    }
    // Otherwise, consider everything that's not identifier-or-comma as a
    // description.
    let first_non_ident = rest
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == ',' || *c == ' '))
        .map(|(i, _)| i);
    if let Some(i) = first_non_ident {
        !rest[i..].trim().is_empty()
    } else {
        // No description part at all.
        false
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for block in find_jsdoc_blocks(ctx.source) {
            for tag in parse_tags(&block.content) {
                if tag.name != "template" {
                    continue;
                }
                if !template_has_description(&tag.value) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: block.start_line + tag.line_offset + 1,
                        column: 1,
                        rule_id: "jsdoc/require-template-description".into(),
                        message: "@template tag is missing a description — document the type parameter.".into(),
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
    fn flags_template_without_description() {
        let src = "/**\n * @template T\n */\nfunction id<T>(x: T): T { return x; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_template_with_constraint_but_no_description() {
        let src = "/**\n * @template {string} K\n */\nfunction f<K extends string>(x: K): K { return x; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_template_with_description() {
        let src = "/**\n * @template T - the element type\n */\nfunction id<T>(x: T): T { return x; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_multiple_templates_with_description() {
        let src = "/**\n * @template T, U - a pair\n */\nfunction f<T, U>(t: T, u: U): [T, U] { return [t, u]; }";
        assert!(run(src).is_empty());
    }
}

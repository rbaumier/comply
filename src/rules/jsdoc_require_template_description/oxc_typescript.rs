//! jsdoc/require-template-description oxc backend — every `@template` tag
//! needs a description.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, parse_tags, strip_type_annotation};
use std::sync::Arc;

fn template_has_description(value: &str) -> bool {
    let rest = strip_type_annotation(value.trim());
    if let Some(idx) = rest.find(" - ") {
        return !rest[idx + 3..].trim().is_empty();
    }
    let first_non_ident = rest
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == ',' || *c == ' '))
        .map(|(i, _)| i);
    if let Some(i) = first_non_ident {
        !rest[i..].trim().is_empty()
    } else {
        false
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // Scan source for JSDoc blocks manually (comments are not in the oxc AST).
        let src = ctx.source;
        let mut search_from = 0;
        while let Some(start) = src[search_from..].find("/**") {
            let abs_start = search_from + start;
            let Some(end_rel) = src[abs_start + 3..].find("*/") else { break };
            let abs_end = abs_start + 3 + end_rel + 2;
            let comment_text = &src[abs_start..abs_end];
            let line_offset = byte_offset_to_line_col(src, abs_start).0 - 1;

            for block in find_jsdoc_blocks(comment_text) {
                for tag in parse_tags(&block.content) {
                    if tag.name != "template" {
                        continue;
                    }
                    if !template_has_description(&tag.value) {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: block.start_line + tag.line_offset + 1 + line_offset,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: "@template tag is missing a description — document the type parameter.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
            search_from = abs_end;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
        let src =
            "/**\n * @template T - the element type\n */\nfunction id<T>(x: T): T { return x; }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_multiple_templates_with_description() {
        let src = "/**\n * @template T, U - a pair\n */\nfunction f<T, U>(t: T, u: U): [T, U] { return [t, u]; }";
        assert!(run(src).is_empty());
    }
}

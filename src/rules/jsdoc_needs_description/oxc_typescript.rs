//! OxcCheck backend for jsdoc-needs-description.
//!
//! JSDoc comments are not AST nodes in oxc, so we scan the source text
//! directly via `run_on_semantic`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_type_only_tag(tag: &str) -> bool {
    matches!(
        tag,
        "type"
            | "param"
            | "arg"
            | "argument"
            | "returns"
            | "return"
            | "template"
            | "typedef"
            | "callback"
            | "property"
            | "prop"
            | "this"
            | "implements"
            | "extends"
            | "satisfies"
    )
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let src = ctx.source;

        // Find all `/**` comment blocks in the source.
        let mut search_from = 0;
        while let Some(start) = src[search_from..].find("/**") {
            let abs_start = search_from + start;
            let Some(end_rel) = src[abs_start..].find("*/") else { break };
            let abs_end = abs_start + end_rel + 2;
            let block = &src[abs_start..abs_end];

            search_from = abs_end;

            let mut tags: Vec<&str> = Vec::new();
            let mut has_description = false;

            for line in block.lines() {
                let trimmed = line.trim();
                let content = trimmed
                    .trim_start_matches("/**")
                    .trim_start_matches("*/")
                    .trim_start_matches('*')
                    .trim_end_matches("*/")
                    .trim();

                if content.is_empty() || content == "/" {
                    continue;
                }

                if content.starts_with('@') {
                    if let Some(tag) = content
                        .trim_start_matches('@')
                        .split_whitespace()
                        .next()
                    {
                        tags.push(tag);
                    }
                } else {
                    has_description = true;
                }
            }

            if !tags.is_empty() && !has_description && !tags.iter().all(|tag| is_type_only_tag(tag)) {
                let (line, column) = byte_offset_to_line_col(src, abs_start);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "JSDoc block contains only tags — add a prose description explaining what this does and why.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn allows_param_return_type_signature() {
        let source = r#"
/**
 * @param x - the input
 * @returns the output
 */
function foo(x: number): number { return x; }
"#;
        assert!(run_on(source).is_empty());
    }


    #[test]
    fn allows_type_only_jsdoc_annotation() {
        let source = r#"
/** @type {import('@sveltejs/kit').Config} */
const config = {};
"#;
        assert!(run_on(source).is_empty());
    }


    #[test]
    fn flags_single_line_tag_only() {
        let source = "/** @deprecated */\nfunction old() {}";
        let d = run_on(source);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_jsdoc_with_description() {
        let source = r#"
/**
 * Computes the square of a number.
 * @param x - the input
 * @returns the squared value
 */
function square(x: number): number { return x * x; }
"#;
        assert!(run_on(source).is_empty());
    }


    #[test]
    fn allows_jsdoc_with_description_only() {
        let source = r#"
/**
 * This function does something important.
 */
function important() {}
"#;
        assert!(run_on(source).is_empty());
    }


    #[test]
    fn allows_empty_jsdoc() {
        let source = r#"
/**
 */
function foo() {}
"#;
        assert!(run_on(source).is_empty());
    }
}

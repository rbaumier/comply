//! OxcCheck backend for jsdoc-needs-description.
//!
//! JSDoc comments are not AST nodes in oxc, so we scan the source text
//! directly via `run_on_semantic`.
//!
//! A block is flagged when it has tags but no prose description, unless every
//! tag is type-only (the type is the documentation) or a JSX compiler pragma
//! (`@jsx`, `@jsxImportSource`, `@jsxRuntime`, `@jsxFrag`), where the whole
//! comment is a compiler directive with no prose to add.
//!
//! A `@description`/`@desc` tag is itself the prose description, and a
//! `@deprecated` tag carrying an inline reason supplies the prose, so blocks
//! using either as the description are not flagged.

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
            | "typeparam"
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

/// JSX compiler pragma directives (Babel/TypeScript) carried in JSDoc syntax.
/// The whole comment is the directive consumed by the compiler — there is no
/// prose description to add.
fn is_pragma_tag(tag: &str) -> bool {
    matches!(tag, "jsx" | "jsxImportSource" | "jsxRuntime" | "jsxFrag")
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
                    let rest = content.trim_start_matches('@');
                    if let Some(tag) = rest.split_whitespace().next() {
                        // `@description`/`@desc` supply the prose description by
                        // definition; `@deprecated <reason>` carries the prose
                        // inline. Either counts as the block's description.
                        let inline_text = rest[tag.len()..].trim();
                        match tag {
                            "description" | "desc" => has_description = true,
                            "deprecated" if !inline_text.is_empty() => has_description = true,
                            _ => {}
                        }
                        tags.push(tag);
                    }
                } else {
                    has_description = true;
                }
            }

            if !tags.is_empty()
                && !has_description
                && !tags
                    .iter()
                    .all(|tag| is_type_only_tag(tag) || is_pragma_tag(tag))
            {
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_description_tag() {
        // Regression for rbaumier/comply#4729 — `@description` IS the prose.
        let source = r#"
/**
 * @description title of the tab
 */
const label = { type: String, default: '' };
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_desc_shorthand_tag() {
        // Regression for rbaumier/comply#4729 — `@desc` is the older shorthand.
        let source = r#"
/**
 * @desc Determine if target element is focusable
 * @param element {HTMLElement}
 * @returns {Boolean} true if it is focusable
 */
function isFocusable(element: HTMLElement) {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_deprecated_with_inline_reason() {
        // Regression for rbaumier/comply#4729 — `@deprecated <reason>` carries
        // the prose explanation inline.
        let source = r#"
/**
 * @deprecated Removed after 3.0.0, Use `TabPaneProps` instead.
 */
export const tabPaneProps = {};
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_bare_deprecated_without_reason() {
        // A `@deprecated` with no inline reason supplies no prose.
        let source = r#"
/**
 * @deprecated
 * @returns {void}
 */
function legacy() {}
"#;
        assert!(!run_on(source).is_empty());
    }

    #[test]
    fn flags_tags_only_block() {
        let source = r#"
/**
 * @see other
 * @author someone
 */
function thing() {}
"#;
        assert!(!run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_prose() {
        let source = r#"
/**
 * Does the thing.
 * @see other
 */
function thing() {}
"#;
        assert!(run_on(source).is_empty());
    }
}

//! jsdoc/check-property-names OxcCheck backend — flag duplicate `@property`
//! names inside `@typedef` JSDoc blocks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use std::collections::HashSet;
use std::sync::Arc;

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
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for comment in semantic.comments() {
            let raw = &ctx.source[comment.span.start as usize..comment.span.end as usize];
            if !raw.starts_with("/**") {
                continue;
            }

            let comment_start_offset = comment.span.start as usize;

            for block in scan_blocks(raw) {
                let tags = block.tags();
                let is_typedef = tags.iter().any(|t| {
                    matches!(t.name.as_str(), "typedef" | "interface" | "namespace")
                });
                if !is_typedef {
                    continue;
                }
                let mut seen: HashSet<String> = HashSet::new();
                for tag in tags
                    .iter()
                    .filter(|t| matches!(t.name.as_str(), "property" | "prop"))
                {
                    let Some(name) = property_name(&tag.body) else {
                        continue;
                    };
                    if !seen.insert(name.clone()) {
                        // Find the byte offset of this tag's line within the comment.
                        let tag_byte_offset = find_tag_line_offset(raw, tag.line);
                        let (line, column) = byte_offset_to_line_col(
                            ctx.source,
                            comment_start_offset + tag_byte_offset,
                        );
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Duplicate @property name `{name}` on a @typedef — remove or rename the duplicate."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        }

        diagnostics
    }
}

/// Find the byte offset of a given line number (0-based) within the comment text.
fn find_tag_line_offset(text: &str, line: usize) -> usize {
    let mut current_line = 0;
    for (i, c) in text.char_indices() {
        if current_line == line {
            return i;
        }
        if c == '\n' {
            current_line += 1;
        }
    }
    0
}

/// Extract the property name from a `@property` tag body.
fn property_name(body: &str) -> Option<String> {
    let after_type = strip_leading_type(body);
    let first = after_type.split_whitespace().next()?;
    let cleaned = first.trim_start_matches('[').trim_end_matches(']');
    let name = cleaned.split('=').next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// If `body` starts with `{...}`, return the rest (with leading
/// whitespace stripped). Otherwise return `body` unchanged.
fn strip_leading_type(body: &str) -> &str {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('{') {
        return trimmed;
    }
    let mut depth = 0usize;
    for (i, ch) in trimmed.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return trimmed[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    trimmed
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_duplicate_property_names() {
        let src = r#"
/**
 * @typedef {Object} Point
 * @property {number} x - x
 * @property {number} x - duplicate
 */
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Duplicate"));
    }

    #[test]
    fn allows_unique_property_names() {
        let src = r#"
/**
 * @typedef {Object} Point
 * @property {number} x
 * @property {number} y
 */
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn only_policies_typedef_blocks() {
        let src = r#"
/**
 * @param {number} x
 * @param {number} x
 */
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn handles_optional_bracketed_names() {
        let src = r#"
/**
 * @typedef {Object} Opt
 * @property {number} [x]
 * @property {number} [x=1]
 */
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}

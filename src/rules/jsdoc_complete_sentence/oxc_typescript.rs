//! jsdoc-complete-sentence OxcCheck backend — JSDoc descriptions must start
//! with a capital letter and end with punctuation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Extract description prose lines from a JSDoc comment text.
///
/// The description is the prose **before the first `@tag`**. Once a
/// `@tag` line is seen, everything after it (including the body of
/// `@example` code blocks, `@param` descriptions, etc.) is no longer
/// part of the description. Those bodies follow their own conventions
/// — code in `@example` ends with `;`, `)`, `}`, not with `.`.
///
/// Markdown fenced code blocks (```` ```lang `` … `` ``` ````) embedded
/// in the description — e.g. an inline `Example:` section without an
/// `@example` tag — are not prose: the fence delimiters and the code
/// inside them are excluded, so the closing fence is never treated as
/// the last sentence requiring terminal punctuation. The label that
/// introduces such a block (a line ending in `:`, like `Example:`) is a
/// code-block heading, not a concluding sentence, so it too is dropped.
fn extract_description_lines(text: &str) -> Vec<(String, usize)> {
    let mut description_lines = Vec::new();
    let mut in_fence = false;

    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let content = trimmed
            .trim_start_matches("/**")
            .trim_start_matches("*/")
            .trim_start_matches('*')
            .trim_end_matches("*/")
            .trim();

        if content.starts_with("```") {
            if !in_fence {
                drop_trailing_code_block_label(&mut description_lines);
            }
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if content.starts_with('@') {
            break;
        }
        if content.is_empty() || content == "/" {
            continue;
        }

        description_lines.push((content.to_string(), i));
    }

    description_lines
}

/// Drop a trailing `:`-terminated label that introduces a fenced code
/// block (e.g. `Example:`). Such a heading is not the concluding
/// sentence of the description, so it must not gate the punctuation
/// check.
fn drop_trailing_code_block_label(description_lines: &mut Vec<(String, usize)>) {
    if description_lines
        .last()
        .is_some_and(|(text, _)| text.trim_end().ends_with(':'))
    {
        description_lines.pop();
    }
}

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
            let description_lines = extract_description_lines(raw);
            if description_lines.is_empty() {
                continue;
            }

            // First line must start with a capital letter.
            let (first_text, first_offset) = &description_lines[0];
            if let Some(ch) = first_text.chars().next()
                && ch.is_alphabetic() && !ch.is_uppercase() {
                    let line_byte_offset =
                        find_line_byte_offset(raw, *first_offset);
                    let (line, column) = byte_offset_to_line_col(
                        ctx.source,
                        comment_start_offset + line_byte_offset,
                    );
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "JSDoc description must start with a capital letter.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }

            // Last line must end with punctuation.
            let (last_text, last_offset) = &description_lines[description_lines.len() - 1];
            if let Some(ch) = last_text.trim_end().chars().last()
                && ch != '.' && ch != '!' && ch != '?' {
                    let line_byte_offset =
                        find_line_byte_offset(raw, *last_offset);
                    let (line, column) = byte_offset_to_line_col(
                        ctx.source,
                        comment_start_offset + line_byte_offset,
                    );
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "JSDoc description must end with `.`, `!`, or `?`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
        }

        diagnostics
    }
}

/// Find the byte offset of a given line number (0-based) within text.
fn find_line_byte_offset(text: &str, line: usize) -> usize {
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
    fn flags_lowercase_start() {
        let source = r#"
/**
 * adds two numbers.
 */
function add(a: number, b: number) {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("capital")));
    }

    #[test]
    fn flags_missing_punctuation() {
        let source = r#"
/**
 * Adds two numbers
 */
function add(a: number, b: number) {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn allows_proper_sentence() {
        let source = r#"
/**
 * Adds two numbers.
 */
function add(a: number, b: number) {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_exclamation() {
        let source = "/** Do not call this directly! */\nfunction internal() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_at_example_code_body() {
        // Regression for rbaumier/comply#24 — @example body ends with `;`
        // (or `)`, `}` ) by design; it must not be checked as prose.
        let source = r#"
/**
 * Authorize a write intent.
 *
 * @example
 * authorize(session, { kind: "createOrganization" }).unwrap();
 */
export function authorize(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_trailing_fenced_code_block_in_description() {
        // Regression for rbaumier/comply#1159 — a description ending with a
        // Markdown fenced code block (an inline `Example:` section without the
        // `@example` tag) must not flag the closing ``` as missing punctuation.
        let source = r#"
/**
 * Create a CommandInfo object that describes a command and its functionality.
 *
 * Example:
 *
 * ```typescript
 * const info = makeCommandInfo('resolve', 'display info');
 * ```
 *
 * @param name the name of this command
 */
export function makeCommandInfo(name: string): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_prose_missing_punctuation_with_no_fence() {
        // The last actual prose sentence lacks terminal punctuation and no
        // code fence is involved — this must still be flagged.
        let source = r#"
/**
 * Adds two numbers together
 */
function add(a: number, b: number) {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn ignores_param_descriptions_after_first_tag() {
        let source = r#"
/**
 * Build a user record.
 *
 * @param name the display name
 * @returns the persisted user
 */
export function build(): void {}
"#;
        // First-tag-and-after is not checked; description ends at the
        // first `@`. "Build a user record." is fine.
        assert!(run_on(source).is_empty());
    }
}

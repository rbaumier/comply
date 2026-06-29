//! jsdoc-complete-sentence OxcCheck backend — JSDoc descriptions must start
//! with a capital letter and end with terminal punctuation. The capital-letter
//! check fires only on a cased lowercase first letter, and the terminal-
//! punctuation check accepts both ASCII (`.`/`!`/`?`) and CJK (`。`/`！`/`？`)
//! terminators, so case-less scripts (Chinese, Japanese, Korean) are not
//! flagged.
//!
//! A `/** … */` comment whose description (the prose before the first `@tag`)
//! is a single line and that documents a property/field signature — a
//! `TSPropertySignature` in an interface or type literal, or a class
//! `PropertyDefinition` — is a field label, not prose, so the terminal-
//! punctuation requirement is not applied to it: idiomatic TypeScript writes
//! such labels as noun phrases without a closing period
//! (`/** Timeout in milliseconds */`). This holds even when `@default`/`@type`/
//! `@example`/`@docs` tags push the raw comment onto multiple lines. The
//! capital-letter check still applies, and a multi-line prose property
//! description, or any function/method doc, still requires a complete sentence.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
///
/// A trailing bare URL (a reference link, e.g. `https://docs.example/`) is
/// not prose either: it ends with a path or query, not with `.`, `!`, or
/// `?`, so it is excluded and never gates the punctuation check.
///
/// A trailing Markdown table (lines starting with `|`) is structural markup,
/// not a prose sentence: its rows are excluded, and the `:`-terminated label
/// that introduces the table is dropped too, so neither gates the check.
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

    drop_trailing_bare_url(&mut description_lines);
    drop_trailing_table_rows(&mut description_lines);
    description_lines
}

/// Drop a trailing line that is a bare URL (e.g. a reference link like
/// `https://docs.example/`). Such a line is not a prose sentence, so it
/// must not gate the terminal-punctuation check. The `:`-terminated label
/// that introduces the link (e.g. `Learn more:`) is a heading for the
/// reference, not a concluding sentence, so it is dropped too.
fn drop_trailing_bare_url(description_lines: &mut Vec<(String, usize)>) {
    if description_lines
        .last()
        .is_some_and(|(text, _)| is_bare_url(text))
    {
        description_lines.pop();
        drop_trailing_code_block_label(description_lines);
    }
}

/// Drop a trailing Markdown table — a run of consecutive lines each starting
/// with `|` (header, separator `| :---: |`, and data rows). Such rows are
/// structural markup, not prose sentences, so they must not gate the terminal-
/// punctuation check. When at least one row is popped, the `:`-terminated label
/// that introduces the table (e.g. `The following table shows the distribution:`)
/// is a heading, not a concluding sentence, so it is dropped too.
fn drop_trailing_table_rows(description_lines: &mut Vec<(String, usize)>) {
    let mut popped_any = false;
    while description_lines
        .last()
        .is_some_and(|(text, _)| text.starts_with('|'))
    {
        description_lines.pop();
        popped_any = true;
    }
    if popped_any {
        drop_trailing_code_block_label(description_lines);
    }
}

/// Whether a line is a bare URL: a single `http(s)://` token with no
/// surrounding prose.
fn is_bare_url(text: &str) -> bool {
    let trimmed = text.trim();
    (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && !trimmed.contains(char::is_whitespace)
}

/// Drop a trailing `:`-terminated label that introduces a non-prose
/// block — a fenced code block (`Example:`) or a reference link
/// (`Learn more:`). Such a heading is not the concluding sentence of the
/// description, so it must not gate the punctuation check.
fn drop_trailing_code_block_label(description_lines: &mut Vec<(String, usize)>) {
    if description_lines
        .last()
        .is_some_and(|(text, _)| text.trim_end().ends_with(':'))
    {
        description_lines.pop();
    }
}

/// Collect the byte-offset start of every property/field-signature node: a
/// `TSPropertySignature` (interface or type-literal member) or a class
/// `PropertyDefinition`. A JSDoc comment with a single-line description
/// immediately preceding one of these documents a field label, not prose, so
/// its terminal-punctuation requirement is waived.
fn property_node_starts(semantic: &oxc_semantic::Semantic) -> Vec<usize> {
    semantic
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::TSPropertySignature(p) => Some(p.span.start as usize),
            AstKind::PropertyDefinition(p) => Some(p.span.start as usize),
            _ => None,
        })
        .collect()
}

/// Whether a JSDoc comment ending at `comment_end` immediately precedes a
/// property/field-signature node — only whitespace separates the comment's end
/// from the node's start. Combined with a single-line description, such a
/// comment is a field label, so the terminal-punctuation check is waived for it.
fn precedes_property(
    comment_end: usize,
    property_starts: &[usize],
    source: &str,
) -> bool {
    property_starts.iter().any(|&start| {
        start >= comment_end && source[comment_end..start].chars().all(char::is_whitespace)
    })
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
        let property_starts = property_node_starts(semantic);

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

            // First line must start with a capital letter. Only a cased
            // lowercase letter (Latin/Cyrillic/Greek `a`-`z`, etc.) is wrong:
            // scripts without letter case (CJK ideographs, Hiragana, Hangul,
            // …) return `false` for `is_lowercase`, so they are not flagged.
            let (first_text, first_offset) = &description_lines[0];
            if let Some(ch) = first_text.chars().next()
                && ch.is_lowercase() {
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

            // Last line must end with punctuation. A description written in a
            // case-less script (CJK ideographs, kana, hangul) is exempt: such
            // text does not use ASCII terminators, and short phrases routinely
            // carry no closing mark at all, so requiring one is meaningless.
            //
            // A `/** … */` comment whose description (the prose before the
            // first `@tag`) is a single line and that documents a property/field
            // signature is a field label, not prose, so the noun-phrase style
            // without a closing period is idiomatic and exempt — even when
            // `@default`/`@type`/`@example`/`@docs` tags make the raw comment
            // span multiple lines. A multi-line prose description on a property,
            // or any non-property doc, still requires terminal punctuation.
            let is_property_label = description_lines.len() == 1
                && precedes_property(comment.span.end as usize, &property_starts, ctx.source);
            let (last_text, last_offset) = &description_lines[description_lines.len() - 1];
            if let Some(ch) = last_text.trim_end().chars().last()
                && !is_terminal_punctuation(ch)
                && !is_caseless_script_letter(ch)
                && !is_property_label {
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

/// Whether a character terminates a sentence. Accepts the ASCII `.`, `!`,
/// `?` and their CJK fullwidth equivalents — the ideographic full stop `。`
/// (U+3002), fullwidth `！` (U+FF01), and fullwidth `？` (U+FF1F) — so a
/// well-formed Chinese/Japanese description is not flagged. The ideographic
/// comma `、` is deliberately excluded: it is a separator, not a terminator.
fn is_terminal_punctuation(ch: char) -> bool {
    matches!(ch, '.' | '!' | '?' | '。' | '！' | '？')
}

/// Whether a character is a letter from a script with no upper/lower case
/// distinction (CJK ideographs, Japanese kana, Korean hangul, Thai, …). Such
/// a character is alphabetic but neither uppercase nor lowercase. Used to
/// exempt non-Latin descriptions from the ASCII terminal-punctuation rule.
fn is_caseless_script_letter(ch: char) -> bool {
    ch.is_alphabetic() && !ch.is_uppercase() && !ch.is_lowercase()
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
    fn ignores_trailing_bare_url_in_description() {
        // Regression for rbaumier/comply#1160 — a description ending with a
        // bare reference URL must not flag the URL line as missing punctuation;
        // URLs end with paths/queries, not `.`, `!`, or `?`.
        let source = r#"
/**
 * Learn more about light and dark modes:
 * https://docs.expo.dev/guides/color-schemes/
 */
export function useThemeColor(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_prose_after_url_line() {
        // Only a *trailing* bare URL is exempt — a URL mid-description followed
        // by a prose sentence missing terminal punctuation must still flag.
        let source = r#"
/**
 * See https://example.com/docs for details
 * but remember to read the notes
 */
function add(a: number, b: number) {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn allows_cjk_description() {
        // Regression for rbaumier/comply#4808 — a Chinese JSDoc description
        // has no Latin capitalization and ends with a phrase (no `.`). CJK
        // ideographs are neither uppercase nor lowercase, so the capital check
        // must not fire; the missing-ASCII-period must not fire either.
        let source = r#"
/** 根据角色动态生成路由 */
function generateRoutes(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_cjk_terminal_punctuation() {
        // An ideographic full stop `。` is a valid sentence terminator.
        let source = "/** 根据角色动态生成路由。 */\nfunction generateRoutes(): void {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_lowercase_latin_with_cjk_fix() {
        // A genuinely incomplete English description (lowercase start, no
        // terminal punctuation) must still be flagged after the CJK fix.
        let source = r#"
/**
 * adds two numbers
 */
function add(a: number, b: number) {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("capital")));
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

    #[test]
    fn allows_single_line_interface_property_label() {
        // Regression for rbaumier/comply#6064 — a single-line `/** … */` noun-
        // phrase label on an interface property is idiomatic TS and must not be
        // flagged for missing terminal punctuation.
        let source = r#"
export interface ExampleFile {
  /** Full absolute path to the example file */
  path: string
  /** Relative path from project root */
  relativePath: string
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_single_line_class_property_label() {
        // A single-line label on a class property is exempt too.
        let source = r#"
class Foo {
  /** Timeout in milliseconds */
  timeout = 0;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_lowercase_single_line_property_label() {
        // The label exemption waives only terminal punctuation — a lowercase
        // start is still flagged.
        let source = r#"
export interface ExampleFile {
  /** full absolute path to the example file */
  path: string
}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("capital")));
    }

    #[test]
    fn still_flags_multi_line_property_doc_missing_punctuation() {
        // A multi-line prose doc on a property is prose, not a label, so it
        // still requires terminal punctuation.
        let source = r#"
export interface ExampleFile {
  /**
   * The absolute path to the example file on disk, resolved relative to the
   * project root before the examples are collected
   */
  path: string
}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn still_flags_function_doc_missing_punctuation() {
        // A single-line doc on a function is prose, not a field label, so it
        // still requires terminal punctuation.
        let source = "/** Adds two numbers */\nfunction add(a: number, b: number) {}";
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn allows_single_line_description_property_label_with_tags() {
        // Regression for rbaumier/comply#6449 — a noun-phrase label on an
        // interface property whose description (before the first `@tag`) is a
        // single line must not be flagged, even though `@default`/`@type`/
        // `@example`/`@docs` tags make the raw comment span multiple lines.
        let source = r#"
export interface ModuleOptions {
  /**
   * Supabase API URL
   * @default process.env.NUXT_PUBLIC_SUPABASE_URL || process.env.SUPABASE_URL
   * @example 'https://*.supabase.co'
   * @type string
   * @docs https://supabase.com/docs/reference/javascript/initializing#parameters
   */
  url: string

  /**
   * Redirect automatically to login page if user is not authenticated
   * @default `true`
   * @type boolean
   */
  redirect?: boolean
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_multi_line_description_property_label_with_tags() {
        // The exemption is only for a single-line description — a multi-line
        // prose description on a property still requires terminal punctuation,
        // even when `@tag` annotations follow it.
        let source = r#"
export interface ModuleOptions {
  /**
   * The absolute path to the example file on disk, resolved relative to the
   * project root before the examples are collected
   * @default ""
   * @type string
   */
  path: string
}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn still_flags_function_doc_with_tags_missing_punctuation() {
        // A single-line description on a function (not a property), even with
        // trailing tags, is prose and still requires terminal punctuation.
        let source = r#"
/**
 * Adds two numbers
 * @param a the first number
 * @returns the sum
 */
function add(a: number, b: number) {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn ignores_trailing_markdown_table_in_description() {
        // Regression for rbaumier/comply#6335 — a description ending with a
        // Markdown table (introduced by a `:`-terminated intro) must not flag
        // the last table row as missing punctuation; table rows are structural
        // markup, not prose, and the intro label is a heading, not a sentence.
        let source = r#"
/**
 * Creates a new function that generates uniformly distributed values.
 *
 * The following table shows the rough distribution:
 *
 * |  Result   | Uniform |
 * | :-------: | ------: |
 * | 0.0 - 0.1 |   10.0% |
 * | 0.9 - 1.0 |   10.0% |
 *
 * @returns A new uniform distributor function.
 */
export function uniform(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_trailing_markdown_table_as_last_content() {
        // A table that is the very last content (no `@tag` after it) is still
        // structural markup and must not gate the punctuation check.
        let source = r#"
/**
 * Lookup table for the supported modes.
 *
 * |  Mode  | Value |
 * | :----: | ----: |
 * | fast   |     0 |
 * | slow   |     1 |
 */
export function modes(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_prose_missing_punctuation_with_no_table() {
        // A plain prose description with no terminal punctuation and no table
        // must still be flagged.
        let source = r#"
/**
 * Generates uniformly distributed values
 */
function uniform(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn still_flags_prose_after_table_missing_punctuation() {
        // Only a *trailing* table is dropped — a table mid-description followed
        // by a concluding prose sentence missing terminal punctuation must still
        // flag.
        let source = r#"
/**
 * Shows the distribution:
 *
 * |  Result   | Uniform |
 * | :-------: | ------: |
 * | 0.0 - 0.1 |   10.0% |
 *
 * but remember to read the notes
 */
function uniform(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }
}

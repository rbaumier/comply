//! jsdoc-complete-sentence OxcCheck backend — JSDoc descriptions must start
//! with a capital letter and end with terminal punctuation. The capital-letter
//! check fires only on a cased lowercase first letter, and is waived when the
//! description opens with a dotted Latin initialism (`e.g.`, `i.e.` — a run of
//! single lowercase letters each followed by a period), which is lowercase by
//! convention. The terminal-punctuation check accepts both ASCII (`.`/`!`/`?`)
//! and CJK (`。`/`！`/`？`) terminators, so case-less scripts (Chinese,
//! Japanese, Korean) are not flagged.
//!
//! A `/** … */` comment whose description (the prose before the first `@tag`)
//! is a single line and that documents a property/field signature — a
//! `TSPropertySignature` in an interface or type literal, an interface method
//! signature (`TSMethodSignature`), or a class `PropertyDefinition` — is a
//! field label, not prose, so the terminal-punctuation requirement is not
//! applied to it: idiomatic TypeScript writes such labels as noun phrases
//! without a closing period (`/** Timeout in milliseconds */`). This holds even
//! when `@default`/`@type`/`@example`/`@docs` tags push the raw comment onto
//! multiple lines. The capital-letter check still applies, and a multi-line
//! prose property description, or any free-standing function or class-method
//! doc, still requires a complete sentence.

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
/// A trailing bare URL (a reference link, e.g. `https://docs.example/`) or a
/// labeled URL reference (e.g. `Source: https://…`) is not prose either: it is
/// a reference citation that ends with a path or query, not with `.`, `!`, or
/// `?`, so it is excluded and never gates the punctuation check.
///
/// A trailing Markdown table (lines starting with `|`) is structural markup,
/// not a prose sentence: its rows are excluded, and the `:`-terminated label
/// that introduces the table is dropped too, so neither gates the check.
///
/// A trailing footnote anchor (`[N]`, e.g. `[1]`) is a reference marker, not a
/// prose sentence: it is the target of an earlier `([N] …)` footnote reference
/// and typically anchors following `@see`/`@link` tags, so it is excluded and
/// never gates the punctuation check.
///
/// A trailing Markdown bullet list (items introduced by a `- ` or `* ` marker)
/// is a labeled enumeration of values or options, not a prose sentence: its
/// items conventionally carry no closing period, so the whole list is excluded
/// and the `:`-terminated label that introduces it is dropped too, exactly like
/// a trailing table.
///
/// A trailing inline HTML/JSX closing tag (`</docs-info>`, `</b>`) is wrapping
/// markup, not prose: it is stripped from the last line so the real last prose
/// character — the one preceding the tag (`…routes.</docs-info>` ends at `.`) —
/// gates the check. Nested wrappers are unwrapped fully.
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
    drop_trailing_footnote_anchor(&mut description_lines);
    drop_trailing_table_rows(&mut description_lines);
    drop_trailing_list_items(&mut description_lines);
    drop_trailing_html_tag(&mut description_lines);
    description_lines
}

/// Drop a trailing line that is a bare URL (e.g. a reference link like
/// `https://docs.example/`) or a labeled URL reference (e.g.
/// `Source: https://en.wikipedia.org/wiki/…`). Such a line is a reference
/// citation, not a prose sentence, so it must not gate the terminal-
/// punctuation check. The `:`-terminated label that introduces a bare-URL
/// line (e.g. `Learn more:`) is a heading for the reference, not a
/// concluding sentence, so it is dropped too.
fn drop_trailing_bare_url(description_lines: &mut Vec<(String, usize)>) {
    if description_lines
        .last()
        .is_some_and(|(text, _)| is_bare_url(text) || is_labeled_url(text))
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

/// Drop a trailing Markdown bullet list — a run of list items, each introduced
/// by a `- ` or `* ` marker and optionally wrapped onto following continuation
/// lines. List items are a labeled enumeration of values/options, not prose
/// sentences, and conventionally carry no closing period, so they must not gate
/// the terminal-punctuation check. The list is only dropped when the description
/// actually ends with a bullet item; everything from the first item of that
/// trailing list onward is removed, and the `:`-terminated label that introduces
/// it (e.g. `The value is one of:`) is dropped too, exactly like a trailing
/// table. Prose preceding the list — including a sentence that itself lacks a
/// closing period — is left intact and still checked.
fn drop_trailing_list_items(description_lines: &mut Vec<(String, usize)>) {
    if !description_lines
        .last()
        .is_some_and(|(text, _)| is_bullet_item(text))
    {
        return;
    }
    if let Some(start) = description_lines
        .iter()
        .position(|(text, _)| is_bullet_item(text))
    {
        description_lines.truncate(start);
        drop_trailing_code_block_label(description_lines);
    }
}

/// Strip a trailing inline HTML/JSX closing tag (`</identifier>`) from the last
/// description line. Wrapping markup is structure, not prose, so a closing tag at
/// the end of the line must not gate the terminal-punctuation check: the real
/// last prose character is the one preceding the tag (`…routes.</docs-info>` ends
/// the sentence at `.`). Nested wrappers (`text.</b></docs-info>`) are unwrapped
/// by stripping repeatedly. Only a well-formed `</[A-Za-z][\w-]*>` close tag is
/// removed — a `>` that is not part of a close tag (a `=>` arrow, a `>`
/// comparison) is left intact — so a description that genuinely lacks terminal
/// punctuation before the tag (`…routes</docs-info>`) is still flagged. If the
/// line consists solely of the tag(s), it is dropped so the prose line beneath
/// gates the check.
fn drop_trailing_html_tag(description_lines: &mut Vec<(String, usize)>) {
    let Some((text, _)) = description_lines.last_mut() else {
        return;
    };
    let mut end = text.len();
    let mut stripped_any = false;
    while let Some(prefix) = strip_trailing_close_tag(&text[..end]) {
        end = prefix.len();
        stripped_any = true;
    }
    if !stripped_any {
        return;
    }
    text.truncate(end);
    if text.trim().is_empty() {
        description_lines.pop();
    }
}

/// If `s` (ignoring trailing whitespace) ends with a well-formed HTML/JSX closing
/// tag `</identifier>`, return the slice of `s` preceding that tag; otherwise
/// `None`. `identifier` is matched by shape (`[A-Za-z][\w-]*`), not against a
/// fixed set of tag names.
fn strip_trailing_close_tag(s: &str) -> Option<&str> {
    let trimmed = s.trim_end();
    let body = trimmed.strip_suffix('>')?;
    let tag_start = body.rfind("</")?;
    if is_html_tag_name(&body[tag_start + 2..]) {
        Some(&trimmed[..tag_start])
    } else {
        None
    }
}

/// Whether `name` is a valid HTML/JSX tag name: an ASCII letter followed by ASCII
/// word characters or hyphens (`[A-Za-z][\w-]*`).
fn is_html_tag_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Whether a line is a Markdown bullet-list item: after trimming leading
/// whitespace it begins with a `- ` or `* ` marker. The trailing space is
/// required, so a hyphen-led token (`-5`, `--flag`) or an emphasis asterisk
/// (`*emphasis*`) is not mistaken for a bullet.
fn is_bullet_item(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("- ") || t.starts_with("* ")
}

/// Drop a trailing footnote-anchor line (`[N]`) — the target of an earlier
/// `([N] …)` footnote reference, typically anchoring following `@see`/`@link`
/// reference tags. It is a structural marker, not the concluding sentence.
fn drop_trailing_footnote_anchor(description_lines: &mut Vec<(String, usize)>) {
    if description_lines
        .last()
        .is_some_and(|(text, _)| is_footnote_anchor(text))
    {
        description_lines.pop();
    }
}

/// Whether a line is a footnote anchor: a bracket-delimited run of ASCII
/// digits (e.g. `[1]`, `[12]`). Such a line is a reference marker, not a
/// prose sentence, so it must not gate the terminal-punctuation check.
fn is_footnote_anchor(text: &str) -> bool {
    let t = text.trim();
    t.starts_with('[')
        && t.ends_with(']')
        && t.len() > 2
        && t[1..t.len() - 1].bytes().all(|b| b.is_ascii_digit())
}

/// Whether a line is a bare URL: a single `http(s)://` token with no
/// surrounding prose.
fn is_bare_url(text: &str) -> bool {
    let trimmed = text.trim();
    (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && !trimmed.contains(char::is_whitespace)
}

/// Whether a line is a labeled URL reference: a `<label>: <url>` form whose
/// value after the first `": "` is a bare URL (`Source: https://...`,
/// `See: https://...`). A reference citation, not a prose sentence, so it must
/// not gate the terminal-punctuation check.
fn is_labeled_url(text: &str) -> bool {
    text.split_once(": ")
        .is_some_and(|(label, rest)| !label.trim().is_empty() && is_bare_url(rest))
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
/// `TSPropertySignature` (interface or type-literal member), an interface
/// method signature (`TSMethodSignature`), or a class `PropertyDefinition`. A
/// JSDoc comment with a single-line description immediately preceding one of
/// these documents a field label, not prose, so its terminal-punctuation
/// requirement is waived.
fn property_node_starts(semantic: &oxc_semantic::Semantic) -> Vec<usize> {
    semantic
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::TSPropertySignature(p) => Some(p.span.start as usize),
            AstKind::PropertyDefinition(p) => Some(p.span.start as usize),
            AstKind::TSMethodSignature(m) => Some(m.span.start as usize),
            _ => None,
        })
        .collect()
}

/// Whether a JSDoc comment ending at `comment_end` immediately precedes a
/// property/field-signature or interface-method-signature node — only
/// whitespace separates the comment's end from the node's start. Combined with
/// a single-line description, such a comment is a field label, so the
/// terminal-punctuation check is waived for it.
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
            // …) return `false` for `is_lowercase`, so they are not flagged. A
            // description opening with a dotted Latin initialism (`e.g.`,
            // `i.e.`) is exempt: such abbreviations are lowercase by convention.
            let (first_text, first_offset) = &description_lines[0];
            if let Some(ch) = first_text.chars().next()
                && ch.is_lowercase()
                && !starts_with_dotted_abbreviation(first_text) {
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
            // signature or an interface method signature is a field label, not
            // prose, so the noun-phrase style without a closing period is
            // idiomatic and exempt — even when `@default`/`@type`/`@example`/
            // `@docs` tags make the raw comment span multiple lines. A
            // multi-line prose description, or any non-label doc, still requires
            // terminal punctuation.
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

/// Whether a description opens with a dotted Latin initialism: a leading run of
/// single lowercase ASCII letters each followed by a period, repeated at least
/// twice (`e.g.`, `i.e.`, and any longer form like `a.b.c.`). Such
/// abbreviations are lowercase by convention and are never capitalized in
/// technical prose, so a description that begins with one is exempt from the
/// capital-letter check. The shape is purely structural — no ordinary English
/// sentence opens with `<letter>.<letter>.` — so it does not mask a genuine
/// lowercase-word sentence start (`returns the value`), which is still flagged.
///
/// Two-letter abbreviations like `cf.` are deliberately not covered: a short
/// lowercase token followed by a single period is structurally
/// indistinguishable from a real lowercase sentence start, so matching it would
/// suppress genuine violations.
fn starts_with_dotted_abbreviation(desc: &str) -> bool {
    let mut chars = desc.chars();
    let mut segments = 0;
    loop {
        match (chars.next(), chars.next()) {
            (Some(letter), Some('.')) if letter.is_ascii_lowercase() => segments += 1,
            _ => break,
        }
    }
    segments >= 2
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

    #[test]
    fn ignores_trailing_labeled_url_after_sentence() {
        // Regression for rbaumier/comply#6338 — a `.`-terminated sentence
        // followed by a `Source: <url>` reference citation must not flag the
        // labeled URL line as missing punctuation.
        let source = r#"
/**
 * Returns a random color space name from the worldwide accepted color spaces.
 * Source: https://en.wikipedia.org/wiki/List_of_color_spaces_and_their_uses
 */
function colorSpace(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_standalone_labeled_url() {
        // Regression for rbaumier/comply#6338 — a description whose only line is
        // a `Source: <url>` reference empties the description, so neither the
        // capital nor the punctuation check fires.
        let source = r#"
/**
 * Source: https://pl.wikipedia.org/wiki/ULIC
 */
function ulic(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_trailing_labeled_url_other_labels() {
        // The drop matches any `<label>: <bare-url>` shape, not a fixed list of
        // label names — `See:` and `Ref:` reference citations are dropped too.
        let see = r#"
/**
 * Parses the input into tokens.
 * See: https://example.com/spec
 */
function parse(): void {}
"#;
        assert!(run_on(see).is_empty());

        let reference = r#"
/**
 * Parses the input into tokens.
 * Ref: https://example.com/spec
 */
function parse(): void {}
"#;
        assert!(run_on(reference).is_empty());
    }

    #[test]
    fn still_flags_prose_missing_punctuation_with_no_url() {
        // A plain prose description with no terminal punctuation and no URL must
        // still be flagged after the labeled-URL fix.
        let source = r#"
/**
 * Returns the value
 */
function getValue(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn still_flags_labeled_non_url_line() {
        // The drop requires the value after `": "` to be a bare URL — a labeled
        // prose line (`Note: this is important`) is not a reference citation and
        // still requires terminal punctuation.
        let source = r#"
/**
 * Returns the value.
 * Note: this is important
 */
function getValue(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn still_flags_empty_label_before_url() {
        // The label before `": "` must be non-empty — a line that is just a
        // colon then a URL is not a `<label>: <url>` citation and is not dropped.
        let source = r#"
/**
 * Returns the value
 * : https://example.com/spec
 */
function getValue(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn ignores_labeled_url_with_port_in_value() {
        // The `": "` split lands on the label boundary, not on a `host:port`
        // colon inside the URL, so a labeled URL with a port is still dropped.
        let source = r#"
/**
 * Connects to the local service.
 * Source: https://localhost:8080/health
 */
function connect(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_prose_after_labeled_url_line() {
        // Only a *trailing* labeled URL is dropped — a labeled URL mid-
        // description followed by a prose sentence missing terminal punctuation
        // must still flag.
        let source = r#"
/**
 * Parses the input into tokens.
 * Source: https://example.com/spec
 * but remember to read the notes
 */
function parse(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn ignores_trailing_footnote_anchor_before_see_tags() {
        // Regression for rbaumier/comply#6353 — a description whose last non-blank
        // line is a bare footnote anchor `[1]` (the target of an earlier
        // `([1] …)` reference), followed by `@see` reference tags, must not flag
        // the anchor as missing punctuation; the concluding sentence above it
        // already ends with `.`.
        let source = r#"
/**
 * Subscribes to React's external store ([1] We don't really care when/how React does it).
 *
 * [1]
 * @see https://react.dev/reference/react/useSyncExternalStore
 * @param _usage the usage marker
 */
function useSyncExternalStore(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_trailing_multi_digit_footnote_anchor() {
        // A multi-digit anchor `[12]` is dropped the same way as `[1]`.
        let source = r#"
/**
 * Subscribes to the store (see footnote [12] for the rationale).
 *
 * [12]
 * @see https://example.com/ref
 */
function subscribe(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_prose_revealed_by_footnote_anchor_drop() {
        // Dropping the trailing `[1]` anchor reveals the genuine concluding
        // prose line, which is still checked — a sentence missing terminal
        // punctuation above the anchor must still flag.
        let source = r#"
/**
 * Subscribes to the store but does not unsubscribe
 *
 * [1]
 * @see https://example.com/ref
 */
function subscribe(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn footnote_anchor_detection_is_shape_only() {
        // The anchor signal is purely the `[<digits>]` shape: an empty bracket,
        // non-digit content, or trailing prose is not an anchor.
        assert!(is_footnote_anchor("[1]"));
        assert!(is_footnote_anchor("[12]"));
        assert!(!is_footnote_anchor("[]"));
        assert!(!is_footnote_anchor("[note]"));
        assert!(!is_footnote_anchor("[1] text"));
    }

    #[test]
    fn allows_single_line_interface_method_signature_label() {
        // Regression for rbaumier/comply#6352 — a single-line `/** … */` noun-
        // phrase label on an interface method signature is a field label, not
        // prose, so it must not be flagged for missing terminal punctuation,
        // exactly like a label on a property signature.
        let source = r#"
interface EffectStore {
    readonly effect: number;
    /** Begin tracking signals used in this component */
    _start(): void;
    /** Stop tracking the signals used in this component */
    f(): void;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_lowercase_single_line_method_signature_label() {
        // The label exemption waives only terminal punctuation — a lowercase
        // start on a method-signature label is still flagged.
        let source = r#"
interface EffectStore {
    /** begin tracking signals used in this component */
    _start(): void;
}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("capital")));
        assert!(!d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn still_flags_multi_line_interface_method_doc_missing_punctuation() {
        // A multi-line JSDoc block on an interface method signature is prose
        // (two description lines), not a single-line label, so it still
        // requires terminal punctuation.
        let source = r#"
interface EffectStore {
    /**
     * Begins tracking the signals used in this component and keeps
     * accumulating them until the effect store is finished
     */
    _start(): void;
}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn allows_dotted_latin_abbreviation_start() {
        // Regression for rbaumier/comply#6911 — a JSDoc description opening with
        // a dotted Latin initialism (`e.g.`, `i.e.`) is lowercase by convention
        // and must not flag the capital-letter check.
        let eg = r#"
export interface ParserOptions {
  /**
   * e.g. platform native elements, e.g. `<div>` for browsers
   */
  isNativeTag?: (tag: string) => boolean
}
"#;
        assert!(run_on(eg).is_empty());

        let ie = r#"
/**
 * i.e. the foo.
 */
function bar(): void {}
"#;
        assert!(!run_on(ie).iter().any(|d| d.message.contains("capital")));
    }

    #[test]
    fn still_flags_lowercase_word_start_not_abbreviation() {
        // The abbreviation exemption is structural — an ordinary lowercase word
        // that is not a dotted initialism still flags the capital-letter check.
        let source = r#"
/**
 * returns the parser options.
 */
function getOptions(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("capital")));
    }

    #[test]
    fn allows_capitalized_description_unchanged() {
        // A properly capitalized description is unaffected by the abbreviation
        // exemption.
        let source = r#"
/**
 * Platform native elements.
 */
function getOptions(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn dotted_abbreviation_detection_is_shape_only() {
        // The exemption signal is purely the `<letter>.<letter>.` shape repeated
        // at least twice: ordinary lowercase words and two-letter abbreviations
        // like `cf.` are not covered.
        assert!(starts_with_dotted_abbreviation("e.g. platform native elements"));
        assert!(starts_with_dotted_abbreviation("i.e. the foo"));
        assert!(starts_with_dotted_abbreviation("a.b.c. and so on"));
        assert!(!starts_with_dotted_abbreviation("returns the value"));
        assert!(!starts_with_dotted_abbreviation("cf. the spec"));
        assert!(!starts_with_dotted_abbreviation("e.g without trailing dot"));
    }

    #[test]
    fn ignores_trailing_bullet_list_in_description() {
        // Regression for rbaumier/comply#6908 — a JSDoc description that is a
        // Markdown bullet list documenting the possible values of a union type
        // must not flag the last bullet item as missing terminal punctuation;
        // bullet items are an enumeration, not a prose sentence.
        let source = r#"
export interface SimpleExpressionNode {
  /**
   * - `null` means the expression is a simple identifier that doesn't need
   *    parsing
   * - `false` means there was a parsing error
   */
  ast?: number
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_bullet_list_with_intro_label() {
        // A `:`-terminated label introducing the bullet list is a heading, not a
        // concluding sentence, so it is dropped together with the list, exactly
        // like a table intro.
        let source = r#"
/**
 * The value is one of:
 * - `a` the first option
 * - `b` the second option
 */
function pick(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bullet_list_with_terminated_intro_sentence() {
        // Prose preceding the bullet list is left intact: a complete intro
        // sentence ending in `.` followed by a bullet list is fine.
        let source = r#"
/**
 * Returns the mode.
 * - fast
 * - slow
 */
function modes(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_prose_missing_punctuation_with_no_list() {
        // An ordinary prose description that simply forgot its trailing period
        // (no bullet list involved) must still flag the terminal-punctuation
        // violation.
        let source = r#"
/**
 * Returns the parsed value
 */
function parse(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn still_flags_prose_before_bullet_list_missing_punctuation() {
        // Only the trailing list is dropped — a genuine prose sentence preceding
        // the list that lacks terminal punctuation is still checked.
        let source = r#"
/**
 * This paragraph is prose and forgot its period
 * - first option
 * - second option
 */
function f(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn bullet_item_detection_is_shape_only() {
        // The bullet signal is purely the `- `/`* ` marker shape (the trailing
        // space is required): a hyphen-led token or an emphasis asterisk is not a
        // bullet.
        assert!(is_bullet_item("- first option"));
        assert!(is_bullet_item("* second option"));
        assert!(!is_bullet_item("-5 degrees"));
        assert!(!is_bullet_item("--flag enabled"));
        assert!(!is_bullet_item("*emphasis* in prose"));
        assert!(!is_bullet_item("plain prose line"));
    }

    #[test]
    fn ignores_terminal_punctuation_inside_trailing_html_close_tag() {
        // Regression for rbaumier/comply#7249 — the concluding sentence ends with
        // `.` but is wrapped in an inline `<docs-info>…</docs-info>` element. The
        // trailing close tag is markup, not prose, so it must not gate the check.
        let source = r#"
/**
 * Returns the current route matches on the page.
 *
 * <docs-info>useMatches only works with a data router, since it knows the full
 * route tree up front and will not match down into any descendant route trees
 * since the router isn't aware of the descendant routes.</docs-info>
 *
 * @public
 */
export function useMatches(): void {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_single_line_html_wrapped_sentence() {
        // A single-line description wrapped in an inline element whose prose ends
        // with terminal punctuation before the close tag is accepted.
        let source = "/** <docs-info>Some sentence.</docs-info> */\nfunction f(): void {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_nested_trailing_html_close_tags() {
        // Nested wrappers are unwrapped fully, revealing the `.` before them.
        let source = "/** Text here.</b></docs-info> */\nfunction f(): void {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_missing_punctuation_before_html_close_tag() {
        // Stripping the close tag reveals prose that genuinely lacks terminal
        // punctuation — this must still flag.
        let source = r#"
/**
 * <docs-info>useMatches will not match down into any descendant route trees
 * since the router isn't aware of the descendant routes</docs-info>
 */
export function useMatches(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn still_flags_missing_punctuation_with_no_html_tag() {
        // A plain description with no tag and no terminal punctuation is
        // unaffected by the close-tag normalizer and still flags.
        let source = r#"
/**
 * Some description without punctuation
 */
function f(): void {}
"#;
        let d = run_on(source);
        assert!(d.iter().any(|d| d.message.contains("end with")));
    }

    #[test]
    fn close_tag_stripping_is_shape_only() {
        // The signal is the well-formed `</identifier>` close-tag shape at the end
        // of the line: an arrow `=>`, a `>` comparison, or an opening tag is not a
        // close tag and is left intact.
        assert_eq!(strip_trailing_close_tag("routes.</docs-info>"), Some("routes."));
        assert_eq!(strip_trailing_close_tag("text.</b></docs-info>"), Some("text.</b>"));
        assert_eq!(strip_trailing_close_tag("</docs-info>"), Some(""));
        assert_eq!(strip_trailing_close_tag("a map a => b"), None);
        assert_eq!(strip_trailing_close_tag("if a <= b =>"), None);
        assert_eq!(strip_trailing_close_tag("compare x > y"), None);
        assert_eq!(strip_trailing_close_tag("an opening <div>"), None);
        assert_eq!(strip_trailing_close_tag("self closing <br/>"), None);
        assert_eq!(strip_trailing_close_tag("empty <>"), None);
        assert_eq!(strip_trailing_close_tag("plain prose"), None);
    }
}

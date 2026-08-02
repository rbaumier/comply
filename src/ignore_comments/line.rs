//! Per-line parsing of suppression directives, comply's own and ESLint's.
//!
//! Splits a single source line into either:
//! - an above-line marker (suppress next line),
//! - a trailing marker (suppress current line),
//! - or nothing (no marker, marker inside a string literal, missing rule id).
//!
//! Both `// comply-ignore: rule-a` and ESLint's
//! `// eslint-disable-line rule-a` / `// eslint-disable-next-line rule-a`
//! land in the same `LineParse`, so the caller resolves their target line
//! identically.
//!
//! Two string defences coexist because the two marker families ask different
//! questions. comply's own marker carries its opener in its text, so it only
//! has to be somewhere outside a string: `is_inside_string_literal` counts
//! unescaped `"` / `'` / `` ` `` quotes before it, and an odd count rejects
//! `const s = "// comply-ignore: ...";`. An ESLint directive has to be the
//! first token of its comment, so `Comments` walks the comments of the line and
//! the directive is read from a comment body, never from the line at large.
//!
//! Both work on one line at a time, so a string or a block comment opened on an
//! earlier line is invisible here. The shape such a line leaves behind is what
//! `is_block_comment_padding` recognises.

use crate::diagnostic::{Diagnostic, Severity};
use crate::ignore_comments::payload;
use std::path::Path;

const MARKER: &str = "// comply-ignore:";
const FILE_MARKER: &str = "// comply-ignore-file:";
const ESLINT_DISABLE_NEXT_LINE: &str = "eslint-disable-next-line";
const ESLINT_DISABLE_LINE: &str = "eslint-disable-line";

/// Outcome of parsing a single source line.
#[derive(Debug)]
pub struct LineParse {
    /// All rule ids referenced by the marker. ESLint-style comma-separated
    /// syntax (`// comply-ignore: rule-a, rule-b — reason`) yields multiple
    /// entries; the single-rule form yields exactly one.
    pub rule_ids: Vec<String>,
    /// Line number to insert into the suppressions map. `None` means the
    /// directive suppresses the rule for the entire file (the new
    /// `// comply-ignore-file:` marker).
    pub target_line: Option<usize>,
    /// True when nothing but the directive comment sits on this line, so a
    /// marker above it forwards past it to the first line carrying code.
    pub is_comment_only_line: bool,
    /// Diagnostic to emit if the comment was missing its justification.
    pub bad_ignore: Option<Diagnostic>,
}

/// Parse one source line. Returns None if no honored directive is present.
pub fn parse(path: &Path, line: &str, line_num: usize) -> Option<LineParse> {
    parse_comply_marker(path, line, line_num).or_else(|| parse_eslint_disable(line, line_num))
}

fn parse_comply_marker(path: &Path, line: &str, line_num: usize) -> Option<LineParse> {
    // Check the file-level marker first — it's a strict superset of the
    // per-line marker text (`// comply-ignore-file:` contains the
    // per-line `// comply-ignore:` as a substring with extra suffix),
    // so we'd misclassify otherwise.
    let (marker_byte, is_file_scope, marker_len) =
        if let Some(b) = line.find(FILE_MARKER) {
            (b, true, FILE_MARKER.len())
        } else {
            (line.find(MARKER)?, false, MARKER.len())
        };
    let prefix = &line[..marker_byte];

    if is_inside_string_literal(prefix) {
        return None;
    }

    let parsed = payload::parse(&line[marker_byte + marker_len..]);
    if parsed.rule_ids.is_empty() {
        return None;
    }

    // File-level marker → no specific target line.
    // Trailing per-line marker (code before it on the same line) →
    //   suppresses THIS line.
    // Above-line marker (only whitespace before it) → suppresses NEXT line.
    let is_trailing = !prefix.trim_start().is_empty();
    let target_line = if is_file_scope {
        None
    } else {
        Some(if is_trailing { line_num } else { line_num + 1 })
    };

    let bad_ignore = if parsed.justification.is_empty() {
        let col = prefix.chars().count();
        // Use the first rule id for the example in the error message —
        // a single placeholder is clearer than enumerating the list.
        Some(make_bad_ignore_diagnostic(
            path,
            line_num,
            col,
            &parsed.rule_ids[0],
        ))
    } else {
        None
    };
    Some(LineParse {
        rule_ids: parsed.rule_ids,
        target_line,
        is_comment_only_line: !is_trailing,
        bad_ignore,
    })
}

/// Parse an ESLint inline disable directive written by the project author:
/// `// eslint-disable-next-line rule-a` targets the next line and
/// `// eslint-disable-line rule-a` the line it sits on, in either comment
/// syntax. ESLint's `-- <description>` suffix is dropped. What remains is read
/// by `payload`, which also accepts comply's own em-dash separator — ESLint
/// does not.
///
/// Like ESLint, the directive counts only as the first token of its comment.
/// Prose that names a directive (`// never write eslint-disable-line rule-a`),
/// a directive the author commented back out, and a directive quoted on the
/// padded continuation line of a block comment are all text. ESLint reads them
/// the same way. Locating the comment before searching for the directive is
/// what keeps a directive name quoted in a string from shadowing a real one
/// later on the line.
///
/// A directive listing no rule suppresses nothing: letting a foreign linter's
/// blanket disable silence every comply rule on a line would hide findings the
/// author never looked at. Such a directive is also what
/// `eslint-comments/no-unlimited-disable` exists to flag.
fn parse_eslint_disable(line: &str, line_num: usize) -> Option<LineParse> {
    let (comment, after_directive, target_line) = Comments::new(line).find_map(|comment| {
        let body = comment.body.trim_start();
        match strip_directive(body, ESLINT_DISABLE_NEXT_LINE) {
            Some(rest) => Some((comment, rest, line_num + 1)),
            None => strip_directive(body, ESLINT_DISABLE_LINE).map(|rest| (comment, rest, line_num)),
        }
    })?;
    let before_comment = &line[..comment.start];
    if is_block_comment_padding(before_comment) {
        return None;
    }
    let rule_list = strip_eslint_description(after_directive);
    // A leading `-` is the head of a separator run the split left behind, never
    // the start of a rule name — the directive names no rule.
    if rule_list.trim_start().starts_with('-') {
        return None;
    }
    let parsed = payload::parse(rule_list);
    if parsed.rule_ids.is_empty() {
        return None;
    }
    Some(LineParse {
        rule_ids: parsed.rule_ids,
        target_line: Some(target_line),
        // Nothing before the comment and nothing after it — no code on the line.
        is_comment_only_line: before_comment.trim().is_empty()
            && line[comment.end..].trim().is_empty(),
        // A justification is comply's own requirement, not ESLint's.
        bad_ignore: None,
    })
}

/// Text after `directive` when a comment body opens on that exact directive.
/// A longer word starting with the same letters is a different token, so the
/// name has to end where the directive does.
fn strip_directive<'a>(body: &'a str, directive: &str) -> Option<&'a str> {
    let rest = body.strip_prefix(directive)?;
    rest.chars().next().is_none_or(char::is_whitespace).then_some(rest)
}

/// True when the only thing before a comment opener is the `*` padding a block
/// comment puts at the start of each of its lines.
///
/// Such a `//` opens no comment — the text sits inside the block — so a
/// directive quoted in an `@example` is an illustration, not an instruction.
/// ESLint reads it as comment text for the same reason. The check is per line,
/// which keeps a `/*` written inside a string or a regex from ever silencing
/// the lines that follow it.
fn is_block_comment_padding(before_comment: &str) -> bool {
    let trimmed = before_comment.trim();
    !trimmed.is_empty() && trimmed.bytes().all(|byte| byte == b'*')
}

/// Rule list of an ESLint directive, with its optional trailing description
/// removed.
///
/// ESLint separates the description from the rule list with a run of two or
/// more `-`, surrounded by whitespace. The run must be recognised whatever its
/// length, because the description is prose and regularly names rules: read as
/// a rule list it would suppress findings the author only mentioned. The
/// surrounding whitespace is what keeps the hyphens inside a rule id
/// (`no-nested-ternary`) out of the match. comply's own marker uses a separator
/// of its own, ` -- ` or an em-dash, parsed in `payload`.
fn strip_eslint_description(rule_list: &str) -> &str {
    let mut cursor = 0;
    while let Some(offset) = rule_list[cursor..].find("--") {
        let run_start = cursor + offset;
        let after_run = rule_list[run_start..].trim_start_matches('-');
        let opens_on_space = rule_list[..run_start]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        let closes_on_space = after_run.chars().next().is_none_or(char::is_whitespace);
        if opens_on_space && closes_on_space {
            return &rule_list[..run_start];
        }
        cursor = run_start + "--".len();
    }
    rule_list
}

/// One comment of a source line.
#[derive(Clone, Copy)]
struct Comment<'a> {
    /// Byte offset of the opener.
    start: usize,
    /// Text between the opener and the closer.
    body: &'a str,
    /// Byte offset just past the comment.
    end: usize,
}

/// The comments of one line, left to right.
///
/// The scan skips string literals, so an opener quoted in code
/// (`const s = "// ...";`) yields no comment, and it resumes after a block
/// comment that closes, so the `//` of `/******/ // rule-a` — the shape a
/// bundler banner writes — is still reached. What a comment holds is never
/// rescanned, which is what keeps a directive commented back out
/// (`// // rule-a`) inside the body of the outer comment, where it reads as
/// prose.
///
/// There is no regex-literal state and no cross-line quote state, so a `/*`
/// inside a regex and an unpaired quote in JSX text can both hide a comment
/// from the scan. That loses a directive; it never invents one.
struct Comments<'a> {
    text: &'a str,
    cursor: usize,
}

impl<'a> Comments<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, cursor: 0 }
    }
}

impl<'a> Iterator for Comments<'a> {
    type Item = Comment<'a>;

    /// Skipping an escape inside a quote advances by two bytes, so the cursor
    /// can land mid-character. That stays sound because every comparison here
    /// is on a raw byte and a UTF-8 continuation byte matches none of them: by
    /// the time a slice is taken, the cursor is back on a character boundary.
    fn next(&mut self) -> Option<Comment<'a>> {
        let bytes = self.text.as_bytes();
        let mut quote: Option<u8> = None;
        while self.cursor < bytes.len() {
            let start = self.cursor;
            let byte = bytes[start];
            self.cursor += 1;
            if let Some(open_quote) = quote {
                match byte {
                    b'\\' => self.cursor += 1,
                    b if b == open_quote => quote = None,
                    _ => {}
                }
                continue;
            }
            match byte {
                b'"' | b'\'' | b'`' => quote = Some(byte),
                b'/' if bytes.get(start + 1) == Some(&b'/') => {
                    self.cursor = self.text.len();
                    return Some(Comment {
                        start,
                        body: &self.text[start + 2..],
                        end: self.text.len(),
                    });
                }
                b'/' if bytes.get(start + 1) == Some(&b'*') => {
                    // A block comment with no closer holds the rest of the line.
                    let body_start = start + 2;
                    let close = self.text[body_start..].find("*/");
                    let body_end = close.map_or(self.text.len(), |at| body_start + at);
                    self.cursor = close.map_or(self.text.len(), |at| body_start + at + 2);
                    return Some(Comment {
                        start,
                        body: &self.text[body_start..body_end],
                        end: self.cursor,
                    });
                }
                _ => {}
            }
        }
        None
    }
}

/// True if the prefix has an unmatched opening quote, suggesting the marker
/// is inside a string literal. Conservative heuristic — catches common cases
/// without parsing JS expression syntax.
fn is_inside_string_literal(prefix: &str) -> bool {
    let mut in_double = false;
    let mut in_single = false;
    let mut in_backtick = false;
    let mut chars = prefix.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next(); // Skip escaped character.
            }
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '`' if !in_double && !in_single => in_backtick = !in_backtick,
            _ => {}
        }
    }
    in_double || in_single || in_backtick
}

/// Construct a diagnostic for a comply-ignore comment missing its justification.
fn make_bad_ignore_diagnostic(
    path: &Path,
    line: usize,
    char_column: usize,
    rule_id: &str,
) -> Diagnostic {
    Diagnostic {
        path: std::sync::Arc::from(path),
        line,
        column: char_column + 1,
        rule_id: "comply-ignore-missing-justification".into(),
        message: format!(
            "comply-ignore without justification — explain why this exception \
             is needed: `// comply-ignore: {rule_id} — <reason>`"
        ),
        severity: Severity::Error,
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn above_line_marker_targets_next_line() {
        let lp = parse(Path::new("t.ts"), "  // comply-ignore: no-throw — ok", 5).unwrap();
        assert_eq!(lp.target_line, Some(6));
    }

    #[test]
    fn trailing_marker_targets_current_line() {
        let lp = parse(
            Path::new("t.ts"),
            "throw err; // comply-ignore: no-throw — legacy",
            5,
        )
        .unwrap();
        assert_eq!(lp.target_line, Some(5));
    }

    #[test]
    fn file_marker_yields_no_target_line() {
        // Regression for rbaumier/comply#27 — file-level rules need a
        // way to be suppressed for the whole file.
        let lp = parse(
            Path::new("t.ts"),
            "// comply-ignore-file: elysia-test-missing-validation — third-party endpoint",
            1,
        )
        .unwrap();
        assert_eq!(lp.target_line, None);
        assert_eq!(lp.rule_ids, vec!["elysia-test-missing-validation"]);
    }

    #[test]
    fn multi_rule_marker_yields_all_rule_ids() {
        // Regression for rbaumier/comply#22 — ESLint-style multi-rule.
        let lp = parse(
            Path::new("t.ts"),
            "throw err; // comply-ignore: no-throw, no-explicit-any — legacy bridge",
            5,
        )
        .unwrap();
        assert_eq!(lp.rule_ids, vec!["no-throw", "no-explicit-any"]);
        assert_eq!(lp.target_line, Some(5));
    }

    #[test]
    fn marker_inside_double_quoted_string_is_ignored() {
        assert!(
            parse(
                Path::new("t.ts"),
                "let s = \"// comply-ignore: no-throw — x\";",
                1
            )
            .is_none()
        );
    }

    #[test]
    fn marker_inside_single_quoted_string_is_ignored() {
        assert!(
            parse(
                Path::new("t.ts"),
                "let s = '// comply-ignore: no-throw — x';",
                1
            )
            .is_none()
        );
    }

    #[test]
    fn marker_inside_backtick_template_is_ignored() {
        assert!(
            parse(
                Path::new("t.ts"),
                "let s = `// comply-ignore: no-throw — x`;",
                1
            )
            .is_none()
        );
    }

    #[test]
    fn eslint_disable_next_line_targets_next_line() {
        let lp = parse(Path::new("t.ts"), "  // eslint-disable-next-line no-console", 5).unwrap();
        assert_eq!(lp.target_line, Some(6));
        assert_eq!(lp.rule_ids, vec!["no-console"]);
        assert!(lp.is_comment_only_line);
        assert!(lp.bad_ignore.is_none());
    }

    #[test]
    fn eslint_disable_line_targets_current_line() {
        let lp = parse(Path::new("t.ts"), "console.log(x); // eslint-disable-line no-console", 5)
            .unwrap();
        assert_eq!(lp.target_line, Some(5));
        assert!(!lp.is_comment_only_line);
    }

    #[test]
    fn eslint_disable_keeps_every_listed_rule_and_drops_the_description() {
        let lp = parse(
            Path::new("t.ts"),
            "// eslint-disable-next-line no-console, @typescript-eslint/no-explicit-any -- debug helper",
            1,
        )
        .unwrap();
        assert_eq!(
            lp.rule_ids,
            vec!["no-console", "@typescript-eslint/no-explicit-any"]
        );
    }

    #[test]
    fn eslint_disable_block_form_stops_at_the_comment_end() {
        let lp = parse(
            Path::new("t.ts"),
            "/* eslint-disable-next-line no-console */ const x = 1;",
            1,
        )
        .unwrap();
        assert_eq!(lp.rule_ids, vec!["no-console"]);
        assert_eq!(lp.target_line, Some(2));
        // Code follows the comment, so a marker above this line must stop here
        // instead of forwarding past it.
        assert!(!lp.is_comment_only_line);
    }

    #[test]
    fn eslint_disable_block_form_alone_on_its_line_carries_no_code() {
        let lp = parse(Path::new("t.ts"), "  /* eslint-disable-next-line no-console */  ", 1)
            .unwrap();
        assert!(lp.is_comment_only_line);
    }

    #[test]
    fn eslint_disable_without_a_rule_list_is_not_a_directive() {
        assert!(parse(Path::new("t.ts"), "// eslint-disable-next-line", 1).is_none());
        assert!(parse(Path::new("t.ts"), "x = 1; // eslint-disable-line", 1).is_none());
    }

    #[test]
    fn eslint_disable_description_is_never_read_as_a_rule_list() {
        // The description of a blanket disable often names rules in prose.
        // Reading it as a rule list would suppress a rule the author only
        // mentioned. ESLint accepts any run of two or more `-` as the
        // separator, in any whitespace form.
        for separator in [" -- ", " --- ", "\t--\t", "  ----   "] {
            let line =
                format!("// eslint-disable-next-line{separator}deliberate, no-empty-function");
            assert!(
                parse(Path::new("t.ts"), &line, 1).is_none(),
                "separator `{separator}` should leave no rule list"
            );
        }
    }

    #[test]
    fn eslint_disable_description_is_dropped_after_a_rule_list() {
        let lp = parse(
            Path::new("t.ts"),
            "// eslint-disable-next-line no-console --- see #12, no-empty-function",
            1,
        )
        .unwrap();
        assert_eq!(lp.rule_ids, vec!["no-console"]);
    }

    #[test]
    fn eslint_disable_keeps_a_rule_id_containing_hyphens() {
        // The separator needs whitespace on both sides, so the hyphens inside
        // a rule id never split it.
        let lp = parse(
            Path::new("t.ts"),
            "// eslint-disable-next-line no-nested-ternary",
            1,
        )
        .unwrap();
        assert_eq!(lp.rule_ids, vec!["no-nested-ternary"]);
    }

    #[test]
    fn eslint_disable_on_a_padded_block_comment_line_is_ignored() {
        // The padding is the only trace a block comment leaves on the line it
        // continues, so it is what tells a quoted directive from a real one.
        assert!(
            parse(
                Path::new("t.ts"),
                " * // eslint-disable-next-line no-console",
                1
            )
            .is_none()
        );
        // A `*` next to anything else is code, and an above-line directive has
        // nothing before its opener at all.
        assert!(parse(Path::new("t.ts"), "  // eslint-disable-line no-console", 1).is_some());
        assert!(
            parse(
                Path::new("t.ts"),
                "y = a * b; // eslint-disable-line no-console",
                1
            )
            .is_some()
        );
    }

    #[test]
    fn eslint_disable_is_not_shadowed_by_its_own_name_in_a_string() {
        // The comment is located first, so a directive name quoted in code
        // cannot preempt the real directive that follows it.
        let lp = parse(
            Path::new("t.ts"),
            "const s = \"eslint-disable-next-line\"; // eslint-disable-line no-console",
            5,
        )
        .unwrap();
        assert_eq!(lp.rule_ids, vec!["no-console"]);
        assert_eq!(lp.target_line, Some(5));
    }

    #[test]
    fn eslint_disable_marker_must_end_where_the_directive_name_ends() {
        assert!(parse(Path::new("t.ts"), "// eslint-disable-lineage no-console", 1).is_none());
        assert!(
            parse(
                Path::new("t.ts"),
                "// eslint-disable-next-lineage no-console",
                1
            )
            .is_none()
        );
    }

    #[test]
    fn eslint_disable_outside_a_comment_is_ignored() {
        // No comment opens before the text, so it is data, not an instruction
        // — with no quote around it either, only the comment gate can reject
        // this one.
        assert!(
            parse(
                Path::new("t.tsx"),
                "<p>eslint-disable-next-line no-console</p>",
                1
            )
            .is_none()
        );
        assert!(
            parse(
                Path::new("t.ts"),
                "const s = \"eslint-disable-next-line no-console\";",
                1
            )
            .is_none()
        );
    }

    #[test]
    fn eslint_disable_after_a_string_holding_a_comment_opener_is_honored() {
        // The `//` inside the URL is data; the directive's own opener is the
        // one that counts.
        let lp = parse(
            Path::new("t.ts"),
            "const url = \"http://example.com\"; // eslint-disable-line no-console",
            5,
        )
        .unwrap();
        assert_eq!(lp.rule_ids, vec!["no-console"]);
        assert_eq!(lp.target_line, Some(5));
        assert!(!lp.is_comment_only_line);
    }

    #[test]
    fn eslint_disable_named_in_prose_is_ignored() {
        // The directive counts only as the first token of its comment.
        assert!(
            parse(
                Path::new("t.ts"),
                "// never write eslint-disable-next-line no-console in this file",
                1
            )
            .is_none()
        );
        assert!(
            parse(
                Path::new("t.ts"),
                "/* see eslint-disable-line no-console for the escape hatch */",
                1
            )
            .is_none()
        );
    }

    #[test]
    fn eslint_disable_commented_back_out_is_ignored() {
        assert!(
            parse(
                Path::new("t.ts"),
                "// // eslint-disable-next-line no-console",
                1
            )
            .is_none()
        );
    }

    #[test]
    fn eslint_disable_after_a_closed_block_comment_is_honored() {
        // Bundler output prefixes lines with a closed `/******/` banner; the
        // opener that holds the directive is the `//` that follows it.
        let lp = parse(
            Path::new("t.ts"),
            "/******/ // eslint-disable-next-line no-console",
            1,
        )
        .unwrap();
        assert_eq!(lp.rule_ids, vec!["no-console"]);
        assert_eq!(lp.target_line, Some(2));
    }

    #[test]
    fn eslint_disable_inside_a_string_is_ignored() {
        assert!(
            parse(
                Path::new("t.ts"),
                "const s = \"// eslint-disable-next-line no-console\";",
                1
            )
            .is_none()
        );
    }

    #[test]
    fn file_scope_eslint_disable_is_not_a_line_directive() {
        // Guards the two marker constants: neither may loosen to the range
        // form `eslint-disable rule-a`, which comply does not track.
        assert!(parse(Path::new("t.ts"), "/* eslint-disable no-console */", 1).is_none());
        assert!(parse(Path::new("t.ts"), "// eslint-disable no-console", 1).is_none());
    }
}

//! Miette-powered pretty renderer. Groups diagnostics by file, reads each
//! source file once, and emits a labeled source frame per diagnostic with
//! rule help and doc URL pulled from the RuleMeta registry.
//!
//! Fallbacks:
//! - Diagnostic with `span: None`: uses `span_resolver::resolve_line_span`
//!   to highlight the full line at the reported `(line, column)`.
//! - File unreadable (race, virtual path, deleted between scan and render):
//!   falls back to the eslint-like single line for that diagnostic — no
//!   crash, no error.
//! - `rule_id` absent from the `RuleMeta` registry (delegated oxlint/clippy
//!   diagnostics): help and url omitted; frame still rendered.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::meta::RuleMeta;
use crate::rules::meta_registry;
use miette::{GraphicalReportHandler, GraphicalTheme, LabeledSpan, NamedSource, SourceSpan};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::span_resolver::resolve_line_span;

/// Insert a space between miette's leading box-drawing arc (`╭─`) and
/// the `[path:line:col]` label that follows, so iTerm / wezterm /
/// kitty Smart Selection doesn't glue the arc to the path on
/// cmd+click. Miette emits the two characters adjacent, producing
/// `     ╭─[src/foo.ts:12:3]` — iTerm picks up `╭─[` as part of the
/// path and the resolution fails.
///
/// Tolerant of ANSI color escapes between the arc and the bracket:
/// miette may emit `\x1b[...m` styling, so the helper walks the
/// string and only inserts the space when the next non-ANSI char
/// after `─` is `[`.
#[must_use]
fn unstick_path_from_box_drawing(input: &str) -> String {
    // Fast-path: no arc / tee character → nothing to rewrite.
    if !input.contains('\u{256D}') && !input.contains('\u{251C}') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len() + 8);
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        out.push(c);
        // Looking for `╭─` or `├─` followed by (optional ANSI SGR)
        // followed by `[`. `╭` = U+256D, `─` = U+2500, `├` = U+251C.
        if (c == '\u{256D}' || c == '\u{251C}') && chars.get(i + 1) == Some(&'\u{2500}') {
            // Copy the `─`.
            out.push('\u{2500}');
            i += 2;
            // Skip any ANSI SGR sequences: `ESC [ ... m`.
            while let Some(window) = ansi_sgr_len(&chars[i..]) {
                for k in 0..window {
                    out.push(chars[i + k]);
                }
                i += window;
            }
            // If the next visible char is `[`, insert a space.
            if chars.get(i) == Some(&'[') {
                out.push(' ');
            }
            continue;
        }
        i += 1;
    }
    out
}

/// Return the length of an ANSI SGR escape sequence (`ESC [ ... m`)
/// starting at the head of `chars`, or `None` if `chars` doesn't
/// start with one. Kept permissive — accepts any chars between `[`
/// and the terminating letter, which covers every SGR form miette
/// emits (colors, bold, reset).
fn ansi_sgr_len(chars: &[char]) -> Option<usize> {
    if chars.first() != Some(&'\u{001B}') || chars.get(1) != Some(&'[') {
        return None;
    }
    // Walk until the terminating letter (CSI final byte is any
    // char in 0x40..=0x7E; SGR uses `m`).
    for (k, c) in chars.iter().enumerate().skip(2) {
        if c.is_ascii_alphabetic() {
            return Some(k + 1);
        }
    }
    None
}

#[cfg(test)]
mod unstick_tests {
    use super::unstick_path_from_box_drawing;

    #[test]
    fn inserts_space_between_arc_and_bracket() {
        let input = "   \u{256D}\u{2500}[src/foo.ts:1:1]";
        let got = unstick_path_from_box_drawing(input);
        assert_eq!(got, "   \u{256D}\u{2500} [src/foo.ts:1:1]");
    }

    #[test]
    fn handles_tee_junction_too() {
        // The `├─` variant appears in some miette layouts.
        let input = "\u{251C}\u{2500}[src/foo.ts:1:1]";
        let got = unstick_path_from_box_drawing(input);
        assert_eq!(got, "\u{251C}\u{2500} [src/foo.ts:1:1]");
    }

    #[test]
    fn tolerates_ansi_sgr_between_arc_and_bracket() {
        // miette wraps the `[path:line:col]` in a cyan-bold-underline
        // SGR. The space must go BEFORE the ANSI so the bracket stays
        // adjacent to its styling.
        let input = "\u{256D}\u{2500}\x1b[36;1;4m[src/foo.ts:1:1]\x1b[0m";
        let got = unstick_path_from_box_drawing(input);
        assert_eq!(
            got,
            "\u{256D}\u{2500}\x1b[36;1;4m [src/foo.ts:1:1]\x1b[0m"
        );
    }

    #[test]
    fn leaves_unrelated_input_unchanged() {
        let input = "no box drawing here";
        assert_eq!(unstick_path_from_box_drawing(input), input);
    }

    #[test]
    fn does_not_touch_arc_without_following_bracket() {
        // The closing arc `╰────` ends the frame and isn't followed
        // by a path — we must not alter it.
        let input = "   \u{2570}\u{2500}\u{2500}\u{2500}\u{2500}";
        assert_eq!(unstick_path_from_box_drawing(input), input);
    }

    #[test]
    fn handles_multiple_frames_in_one_buffer() {
        let input = "\u{256D}\u{2500}[a]\ncontext\n\u{256D}\u{2500}[b]";
        let got = unstick_path_from_box_drawing(input);
        assert_eq!(got, "\u{256D}\u{2500} [a]\ncontext\n\u{256D}\u{2500} [b]");
    }
}

/// Render a slice of diagnostics in human-pretty format using miette. Groups
/// by file path so each file's source is read exactly once. Diagnostics whose
/// file can't be read fall back to the eslint-like single-line form for that
/// entry only.
#[must_use]
pub fn render_pretty(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    let handler = GraphicalReportHandler::new().with_theme(GraphicalTheme::unicode());

    // BTreeMap → stable, alphabetical file ordering for reproducible output.
    // Within a file we preserve the caller's original order.
    let mut by_file: BTreeMap<&std::path::Path, Vec<&Diagnostic>> = BTreeMap::new();
    for d in diagnostics {
        by_file.entry(d.path.as_path()).or_default().push(d);
    }

    for (path, diags) in by_file {
        let source_text = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                // File unreadable — fall back to eslint line per diagnostic.
                for d in &diags {
                    super::write_eslint_line(&mut out, d);
                }
                continue;
            }
        };
        let source_arc = Arc::new(source_text);

        for d in diags {
            let span_pair = d
                .span
                .or_else(|| resolve_line_span(source_arc.as_str(), d.line, d.column))
                .unwrap_or((0, 0));
            let md = MietteDiag {
                diag: d,
                meta: meta_registry::lookup(&d.rule_id),
                source: NamedSource::new(path.display().to_string(), Arc::clone(&source_arc)),
                span: SourceSpan::new(span_pair.0.into(), span_pair.1),
            };
            // Writing into a String never fails; `expect` here is safe.
            let mut frame = String::new();
            handler
                .render_report(&mut frame, &md)
                .expect("render_report into String cannot fail");
            out.push_str(&unstick_path_from_box_drawing(&frame));
            out.push('\n');
        }
    }

    out
}

// Thin wrapper so we can hand one Diagnostic at a time to miette's
// GraphicalReportHandler.
// One MietteDiag per diagnostic; the shared Arc<String> makes the source handle cheap to copy.
struct MietteDiag<'a> {
    diag: &'a Diagnostic,
    meta: Option<RuleMeta>,
    source: NamedSource<Arc<String>>,
    span: SourceSpan,
}

impl std::fmt::Debug for MietteDiag<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MietteDiag")
            .field("rule_id", &self.diag.rule_id)
            .finish()
    }
}

impl std::fmt::Display for MietteDiag<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.diag.message)
    }
}

impl std::error::Error for MietteDiag<'_> {}

impl miette::Diagnostic for MietteDiag<'_> {
    fn code<'b>(&'b self) -> Option<Box<dyn std::fmt::Display + 'b>> {
        Some(Box::new(self.diag.rule_id.as_str()))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.diag.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
        })
    }

    fn help<'b>(&'b self) -> Option<Box<dyn std::fmt::Display + 'b>> {
        self.meta
            .as_ref()
            .map(|m| Box::new(m.remediation) as Box<dyn std::fmt::Display + 'b>)
    }

    fn url<'b>(&'b self) -> Option<Box<dyn std::fmt::Display + 'b>> {
        self.meta
            .and_then(|m| m.doc_url)
            .map(|u| Box::new(u) as Box<dyn std::fmt::Display + 'b>)
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source)
    }

    // Label text is omitted — miette draws the message as the error header already; a label would repeat it next to the caret.
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            None,
            self.span,
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use std::path::PathBuf;

    fn write_fixture(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-miette-pretty-tests");
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        let path = dir.join(name);
        std::fs::write(&path, contents).expect("write fixture");
        path
    }

    #[test]
    fn renders_rule_id_message_and_source_frame() {
        let path = write_fixture("fixture_a.ts", "const x = 1;\n");
        let diag = Diagnostic {
            path: path.clone(),
            line: 1,
            column: 7,
            rule_id: "no-weak-cipher".into(), // real rule id in registry
            message: "example message".into(),
            severity: Severity::Warning,
            span: Some((6, 1)),
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("no-weak-cipher"), "rule id missing: {out}");
        assert!(out.contains("example message"), "message missing: {out}");
        assert!(out.contains("const x = 1;"), "source frame missing: {out}");
    }

    #[test]
    fn unreadable_file_falls_back_to_eslint_line() {
        let path = PathBuf::from("/definitely/does/not/exist/xyz/foo.ts");
        let diag = Diagnostic {
            path: path.clone(),
            line: 10,
            column: 5,
            rule_id: "no-weak-cipher".into(),
            message: "msg".into(),
            severity: Severity::Error,
            span: None,
        };
        let out = render_pretty(&[diag]);
        assert!(
            out.contains("foo.ts:10:5: error [no-weak-cipher] msg"),
            "unreadable-file fallback missing: {out}"
        );
    }

    #[test]
    fn unknown_rule_id_still_renders_frame_without_help() {
        let path = write_fixture("fixture_b.ts", "abc\n");
        let diag = Diagnostic {
            path,
            line: 1,
            column: 1,
            rule_id: "not-a-real-rule-id".into(),
            message: "unknown rule message".into(),
            severity: Severity::Warning,
            span: Some((0, 3)),
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("not-a-real-rule-id"));
        assert!(out.contains("unknown rule message"));
    }

    #[test]
    fn diag_without_span_resolves_whole_line() {
        let path = write_fixture("fixture_c.ts", "first\nsecond\n");
        let diag = Diagnostic {
            path,
            line: 2,
            column: 1,
            rule_id: "no-weak-cipher".into(),
            message: "m".into(),
            severity: Severity::Warning,
            span: None,
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("second"), "second line not highlighted: {out}");
    }
}

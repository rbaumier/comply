use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Byte offset within `line` of the first Unicode bidi control character —
/// trojan source attack vectors — and the character's UTF-8 length. `None` when
/// the line carries none.
///
/// The position is the payload here: the character is invisible, so a reader
/// who is only told "line 12" has no way to find it.
fn find_bidi_char(line: &str) -> Option<(usize, usize)> {
    line.char_indices()
        .find(|&(_, c)| {
            matches!(
                c,
                '\u{202A}' // LRE — left-to-right embedding
                | '\u{202B}' // RLE — right-to-left embedding
                | '\u{202C}' // PDF — pop directional formatting
                | '\u{202D}' // LRO — left-to-right override
                | '\u{202E}' // RLO — right-to-left override
                | '\u{2066}' // LRI — left-to-right isolate
                | '\u{2067}' // RLI — right-to-left isolate
                | '\u{2068}' // FSI — first strong isolate
                | '\u{2069}' // PDI — pop directional isolate
                | '\u{200F}' // RLM — right-to-left mark
                | '\u{200E}' // LRM — left-to-right mark
            )
        })
        .map(|(offset, c)| (offset, c.len_utf8()))
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "\u{202E}", "\u{202D}", "\u{202A}", "\u{202B}", "\u{202C}", "\u{2066}", "\u{2067}",
            "\u{2068}", "\u{2069}",
        ])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // `split_inclusive` keeps the line terminator, so summing segment
        // lengths gives each line's byte offset in the file exactly — under
        // both LF and CRLF, unlike `lines()`, which drops the terminator.
        let mut line_start = 0usize;
        for segment in ctx.source.split_inclusive('\n') {
            let line = segment.trim_end_matches(['\n', '\r']);
            if let Some((offset_in_line, char_len)) = find_bidi_char(line) {
                diagnostics.push(Diagnostic::at_offset(
                    std::sync::Arc::clone(&ctx.path_arc),
                    ctx.source,
                    (line_start + offset_in_line, char_len),
                    "no-bidi-characters",
                    "Invisible bidi control character detected — potential trojan-source attack.".into(),
                    Severity::Error,
                ));
            }
            line_start += segment.len();
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
    fn flags_rlo_char() {
        // U+202E right-to-left override
        let source = "const x = \u{202E}abc;";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_lri_char() {
        let source = "const x = \u{2066}abc;";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_clean_code() {
        assert!(run("const x = 42;").is_empty());
    }

    #[test]
    fn anchors_on_the_invisible_character_not_the_left_margin() {
        // Regression for rbaumier/comply#8428 — the character is invisible, so
        // column 1 left the reader nothing to search for. Two bidi characters
        // on separate lines must land on their own columns and carry a span
        // that slices exactly the offending character.
        let source = "const a = 1;\nconst b = \u{202E}x;\n";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!((diags[0].line, diags[0].column), (2, 11));
        let (offset, len) = diags[0].span.expect("the anchor carries the character span");
        assert_eq!(&source[offset..offset + len], "\u{202E}");
    }

    #[test]
    fn flags_in_comments_too() {
        // Bidi chars in comments are also suspicious.
        let source = "// \u{202A}normal comment";
        assert_eq!(run(source).len(), 1);
    }
}

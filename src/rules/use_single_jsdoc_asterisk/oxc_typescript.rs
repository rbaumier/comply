//! use-single-jsdoc-asterisk OXC backend.
//!
//! Inside a `/** … */` JSDoc block, every continuation line should begin
//! (after the leading indentation) with a *single* `*`, and the closing
//! line should not place stray asterisks right before `*/`. A leading `**`
//! that is immediately followed by content is the markdown bold idiom and is
//! left alone; only a lone extra asterisk separated from the first one (or a
//! trailing asterisk before the close) is flagged.
//!
//! Ported from Biome `useSingleJsDocAsterisk`:
//! - middle lines: after the first `*`, a second `*` preceded only by
//!   whitespace is the violation; an immediate `**` run is treated as bold and
//!   exempt.
//! - the last line (before `*/`) must not have an asterisk sitting in the
//!   whitespace ahead of the closing marker.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// A flagged JSDoc line: its 0-based index within the block and the 0-based
/// byte column of the offending asterisk on the (untrimmed) physical line.
struct Offender {
    line_index: usize,
    column: usize,
}

fn is_inline_whitespace(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Find a middle line (not first, not last) whose leading `*` is followed by a
/// stray second asterisk. Returns the offending column on the physical line.
fn invalid_line_start(lines: &[&str]) -> Option<Offender> {
    if lines.len() < 2 {
        return None;
    }
    // Skip the first line (the `/**` opener) and the last line (handled by the
    // closing-line check).
    for (line_index, &line) in lines.iter().enumerate().take(lines.len() - 1).skip(1) {
        let trimmed = line.trim_start();
        let bytes = trimmed.as_bytes();
        // Lines that don't start with `*` after indentation are free-form text
        // and are not subject to the single-asterisk rule.
        if bytes.first() != Some(&b'*') {
            continue;
        }

        // Walk the bytes after the leading `*`.
        let mut stray: Option<usize> = None;
        let mut rest = bytes.iter().enumerate().skip(1).peekable();
        let mut flagged_at: Option<usize> = None;
        while let Some((idx, &b)) = rest.next() {
            if b == b'*' {
                // `**` immediately together is the bold idiom — valid. If we
                // had already seen a stray single `*`, that earlier one is the
                // violation; otherwise the line is clean.
                if rest.peek().is_some_and(|&(_, &next)| next == b'*') {
                    if stray.is_some() {
                        flagged_at = Some(idx);
                    }
                    break;
                }
                stray = Some(idx);
                continue;
            }
            if !is_inline_whitespace(b) {
                break;
            }
        }
        let Some(col_in_trimmed) = flagged_at.or(stray) else {
            // This line is clean; keep scanning the remaining middle lines.
            continue;
        };

        // Re-anchor onto the physical line: leading indentation + position.
        let indent = line.len() - trimmed.len();
        return Some(Offender {
            line_index,
            column: indent + col_in_trimmed,
        });
    }
    None
}

/// Detect an asterisk sitting in the whitespace just before the closing `*/`
/// on the last line of the block.
fn invalid_last_line(lines: &[&str]) -> Option<Offender> {
    let line_index = lines.len().checked_sub(1)?;
    let line = lines[line_index].trim_end();
    let bytes = line.as_bytes();
    // The line must end with the `*/` closing marker.
    if bytes.len() < 2 || &bytes[bytes.len() - 2..] != b"*/" {
        return None;
    }

    let mut offending: Option<usize> = None;
    // Walk backwards from just before `*/`.
    for idx in (0..bytes.len() - 2).rev() {
        let b = bytes[idx];
        if b == b'*' {
            offending = Some(idx);
            continue;
        }
        if !is_inline_whitespace(b) {
            break;
        }
    }
    offending.map(|column| Offender { line_index, column })
}

impl Check {
    fn check_block(&self, raw: &str, doc_start: usize, ctx: &CheckCtx) -> Option<Diagnostic> {
        let lines: Vec<&str> = raw.lines().collect();
        let offender = invalid_line_start(&lines).or_else(|| invalid_last_line(&lines))?;

        let (base_line, base_col) = byte_offset_to_line_col(ctx.source, doc_start);
        let line = base_line + offender.line_index;
        let column = if offender.line_index == 0 {
            base_col + offender.column
        } else {
            offender.column + 1
        };

        Some(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "JSDoc comment line should start with a single asterisk.".into(),
            severity: Severity::Warning,
            span: None,
        })
    }
}

/// Resolve a comment span to the byte offset of its `/**` opener and the raw
/// `/** … */` text. Returns `None` for non-JSDoc comments. Tolerates oxc spans
/// that either include or exclude the `/*` delimiter.
fn jsdoc_slice(source: &str, start: usize, end: usize) -> Option<(usize, &str)> {
    if let Some(raw) = source.get(start..end)
        && raw.starts_with("/**")
    {
        return Some((start, raw));
    }
    let doc_start = start.checked_sub(2)?;
    let raw = source.get(doc_start..end)?;
    raw.starts_with("/**").then_some((doc_start, raw))
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            // OXC comment spans include the `/*` opener in some builds and
            // exclude it in others; resolve the real `/**` start either way.
            let Some((doc_start, raw)) = jsdoc_slice(ctx.source, start, end) else {
                continue;
            };
            if let Some(diag) = self.check_block(raw, doc_start, ctx) {
                diagnostics.push(diag);
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    // ── Biome `valid.js` fixtures — must NOT fire ──────────────────────

    #[test]
    fn valid_bold_markdown() {
        let src = "/**\n * **bold**\n */\nfunction f() {}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn valid_not_checked_after_double_asterisk() {
        let src = "/**\n * ** *** ** Not checked after double asterisk\n */\nfunction f() {}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn valid_single_asterisk_at_start_on_closing_line() {
        let src = "/**\n * Valid end, single asterisk at the start */\nfunction f() {}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn valid_double_asterisk_at_start_on_closing_line() {
        let src = "/**\n ** * Valid end, double asterisk at the start */\nfunction f() {}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn valid_freeform_text_line_with_asterisks() {
        let src = "/**\n Asterisk after text * *\n*/\nfunction f() {}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn ignores_plain_block_comment() {
        // Not a JSDoc `/**` block — must be ignored entirely.
        let src = "/* End of comment double asterisk\n **/\nfunction f() {}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn ignores_line_comment() {
        let src = "// ** not jsdoc\nfunction f() {}";
        assert!(run(src).is_empty());
    }

    // ── Biome `invalid.js` fixtures — must fire exactly once each ───────

    #[test]
    fn invalid_double_asterisk_closing_marker() {
        let src = "/**\n * End of comment double asterisk\n **/\nfunction f() {}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn invalid_double_asterisk_middle_line() {
        let src = "/**\n * \n ** Middle\n */\nfunction f() {}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn invalid_lonely_double_asterisk_line() {
        let src = "/**\n * \n **\n *\n */\nfunction f() {}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn invalid_asterisk_next_to_text() {
        let src = "/**\n * \n * *Asterisk right next to the text\n */\nfunction f() {}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn invalid_asterisks_before_closing_on_text_line() {
        let src = "/**\n * Desc.\n *\n abc * **/\nfunction f() {}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn invalid_single_line_double_asterisk_close() {
        let src = "/** @someTag SameLine Double **/\nfunction f() {}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn invalid_single_line_spaced_asterisk_close() {
        let src = "/** SameLine DoubleWithSpace * */\nfunction f() {}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // ── CRLF parity (Biome `invalid-crlf.js`) ──────────────────────────

    #[test]
    fn invalid_crlf_middle_line() {
        let src = "/**\r\n * \r\n ** Middle\r\n */\r\nfunction f() {}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // ── Reporting position ─────────────────────────────────────────────

    #[test]
    fn reports_on_the_offending_line() {
        let src = "/**\n * \n ** Middle\n */\nfunction f() {}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        // `** Middle` sits on the 3rd physical line of the file.
        assert_eq!(diags[0].line, 3);
    }
}

//! regex-no-control-chars OXC backend — flag raw literal control bytes in regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Returns `true` if `pattern` contains a control byte (`0x00`-`0x1f` or `0x7f`)
/// written as a *raw literal byte* in the source — the invisible, pasted-by-mistake
/// case the rule targets.
///
/// A control char expressed as an explicit escape (`\xHH`, `\uHHHH`, `\u{...}`, or a
/// short escape like `\t`/`\n`) is deliberate: the author typed the code point on
/// purpose. Those are not raw bytes and so are never matched here. Any byte preceded
/// by a backslash is part of an escape sequence and is skipped.
fn has_control_chars(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            // Skip the backslash and the escaped char so escape sequences
            // (including `\xHH` / `\u{...}`) are never read as raw bytes.
            i += 2;
            continue;
        }
        if is_control_byte(bytes[i]) {
            return true;
        }
        i += 1;
    }
    false
}

fn is_control_byte(b: u8) -> bool {
    b <= 0x1f || b == 0x7f
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };
        let pattern = re.regex.pattern.text.as_str();
        if !has_control_chars(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Raw control character in regex \u{2014} likely an accidental paste; use an explicit escape (e.g. `\\x1b`) if intended."
                .into(),
            severity: Severity::Error,
            span: None,
        });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // Explicit numeric escapes are deliberate — the author typed the code point on
    // purpose (xterm.js VT/ANSI parsers, sanitizers, etc.). They must NOT be flagged.

    #[test]
    fn allows_explicit_hex_escape_for_ansi_sequence() {
        // xterm.js Win32InputMode.test.ts:17 — deliberate ESC via the hex escape.
        assert!(run("const m = seq.match(/^\\x1b\\[(\\d+);(\\d+)_$/);").is_empty());
    }

    #[test]
    fn allows_explicit_octal_escape() {
        // ESC written as the octal escape 033.
        assert!(run("const re = /\\033\\[0m/;").is_empty());
    }

    #[test]
    fn allows_explicit_unicode_escape() {
        // ESC via the 4-digit unicode escape.
        assert!(run("const re = /\\u001b\\[/;").is_empty());
    }

    #[test]
    fn allows_explicit_unicode_braces_escape() {
        // ESC via the unicode code-point escape, unicode-mode regex.
        assert!(run("const re = /\\u{1b}\\[/u;").is_empty());
    }

    #[test]
    fn flags_raw_literal_esc_byte() {
        // A raw 0x1b byte pasted into the source — invisible, almost always a typo.
        assert_eq!(run("const re = /\x1b/;").len(), 1);
    }

    #[test]
    fn flags_raw_literal_bell_byte() {
        // Raw 0x07 (BEL) byte pasted into the pattern.
        assert_eq!(run("const re = /\x07/;").len(), 1);
    }

    #[test]
    fn allows_ordinary_regex() {
        assert!(run("const re = /^[a-z0-9]+$/;").is_empty());
    }
}

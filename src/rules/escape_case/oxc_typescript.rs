//! OXC backend for escape-case — flag lowercase hex digits in escape sequences.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use regex::Regex;
use std::sync::{Arc, LazyLock};

static RE_ESCAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(x[0-9A-Fa-f]{2}|u[0-9A-Fa-f]{4}|u\{[0-9A-Fa-f]+\})").unwrap());

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[
            oxc_ast::AstType::StringLiteral,
            oxc_ast::AstType::TemplateLiteral,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::StringLiteral(lit) => {
                let text = &ctx.source[lit.span.start as usize..lit.span.end as usize];
                check_escapes(text, lit.span.start as usize, ctx, diagnostics);
            }
            AstKind::TemplateLiteral(tpl) => {
                let text = &ctx.source[tpl.span.start as usize..tpl.span.end as usize];
                check_escapes(text, tpl.span.start as usize, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_escapes(text: &str, byte_start: usize, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    for mat in RE_ESCAPE.find_iter(text) {
        let matched = mat.as_str();
        let body = &matched[1..];

        if !has_lowercase_hex(body) {
            continue;
        }

        // The ESC control character (U+001B) is the ANSI CSI introducer, written
        // `\u001b`/`\x1b`/`\u{1b}` in terminal-output strings and fixtures.
        // Uppercasing its hex is cosmetic and breaks copy-paste fidelity with the
        // captured terminal output, so leave it as written.
        if escape_codepoint(body) == Some(0x1B) {
            continue;
        }

        let prefix = &text[..mat.start()];
        let trailing_bs = prefix.len() - prefix.trim_end_matches('\\').len();
        if trailing_bs % 2 == 1 {
            continue;
        }

        let uppercased = format!("\\{}", uppercase_hex(body));
        let abs_offset = byte_start + mat.start();
        let (line, column) = byte_offset_to_line_col(ctx.source, abs_offset);

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "escape-case".into(),
            message: format!(
                "Use uppercase characters for the value of the escape \
                 sequence: `{matched}` -> `{uppercased}`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn has_lowercase_hex(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_ascii_lowercase() && c.is_ascii_hexdigit())
}

/// Decode the codepoint of an escape body (`xHH`, `uHHHH`, or `u{H...}`).
/// Returns `None` if the hex fails to parse as a u32 (malformed or overflowing).
fn escape_codepoint(body: &str) -> Option<u32> {
    let hex = if let Some(rest) = body.strip_prefix("u{") {
        rest.strip_suffix('}')?
    } else {
        &body[1..]
    };
    u32::from_str_radix(hex, 16).ok()
}

fn uppercase_hex(body: &str) -> String {
    body.chars()
        .map(|c| {
            if c.is_ascii_hexdigit() && c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
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
    fn flags_lowercase_hex_escape() {
        let d = run_on(r#"const a = "\xff";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\xFF"));
    }

    #[test]
    fn flags_lowercase_unicode_escape() {
        let d = run_on(r#"const a = "\u00ff";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\u00FF"));
    }

    #[test]
    fn flags_lowercase_unicode_brace_escape() {
        let d = run_on(r#"const a = "\u{1a2b}";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\u{1A2B}"));
    }

    #[test]
    fn allows_uppercase_escape() {
        assert!(run_on(r#"const a = "\xFF";"#).is_empty());
    }

    #[test]
    fn allows_uppercase_unicode() {
        assert!(run_on(r#"const a = "\u00FF";"#).is_empty());
    }

    #[test]
    fn flags_multiple_on_one_line() {
        let d = run_on(r#"const a = "\xff\u00ab";"#);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_ansi_esc_unicode_escape() {
        // ANSI terminal-output fixture: `\u001b` is the ESC/CSI introducer.
        let d = run_on(r#"const a = "\u001b[37;40mnpm\u001b[0m";"#);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_ansi_esc_hex_escape() {
        assert!(run_on(r#"const a = "\x1b[0m";"#).is_empty());
    }

    #[test]
    fn allows_ansi_esc_brace_escape() {
        assert!(run_on(r#"const a = "\u{1b}[0m";"#).is_empty());
    }

    #[test]
    fn flags_other_lowercase_escape_in_ansi_string() {
        // ESC is exempt, but a genuine mixed-case escape next to it still fires.
        let d = run_on(r#"const a = "\u001b[0m\xFf";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\xFF"));
    }

    #[test]
    fn skips_test_fixture_unicode_escapes() {
        // HuggingFace NLP tokenizer test capturing verbatim Thai model output as
        // lowercase `\uXXXX` escapes (U+0E1E U+0E22) — cosmetic casing, no
        // normalization wanted.
        let src = r#"expect(out).toEqual([{ generated_text: "\u0e1e\u0e22" }]);"#;
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "packages/transformers/tests/pipelines/test_pipelines_text_generation.js",
        );
        assert!(d.is_empty(), "test-fixture escapes should not be flagged");
    }

    #[test]
    fn still_flags_lowercase_escape_in_real_source() {
        // Same lowercase escape in non-test source must still flag.
        let src = r#"const a = "\u0e1e";"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/lib/decode.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\u0E1E"));
    }
}

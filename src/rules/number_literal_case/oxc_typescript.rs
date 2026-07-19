//! number-literal-case — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

/// The canonical form: lowercase prefix/exponent (`0x`, `1e3`), with hex digits
/// left in whatever *consistent* case they were written — both `0xff` and `0xFF`
/// pass; only mixed-case digits (`0xfF`) are normalised. Hex digit case is a
/// formatter's concern and formatters disagree (Prettier/oxfmt lower-case them,
/// the unicorn convention upper-cases them), so the rule does not impose one
/// (#5980); this mirrors the Rust backend.
fn canonical(raw: &str) -> Option<String> {
    let (body, suffix) = if let Some(stripped) = raw.strip_suffix('n') {
        (stripped, "n")
    } else {
        (raw, "")
    };

    if body.len() < 2 {
        return None;
    }

    let prefix_lower = body[..2].to_lowercase();
    let fixed = match prefix_lower.as_str() {
        "0x" => {
            // Accept either consistent digit case (`0xff` from Prettier/oxfmt,
            // `0xFF` from the unicorn convention / spec notation); flag only
            // genuinely mixed-case digits (`0xfF`), which no formatter emits. The
            // prefix is still normalised to lowercase. Resolves #5980 without
            // reopening #5797.
            let digits = &body[2..];
            let has_upper = digits.chars().any(|c| c.is_ascii_uppercase());
            let has_lower = digits.chars().any(|c| c.is_ascii_lowercase());
            let normalised = if has_upper && has_lower {
                digits.to_uppercase()
            } else {
                digits.to_string()
            };
            format!("0x{normalised}{suffix}")
        }
        "0b" | "0o" => {
            format!("{}{}{}", prefix_lower, &body[2..], suffix)
        }
        _ => {
            if !body.contains('E') && !body.contains('e') {
                return None;
            }
            let lowered = body.to_lowercase();
            format!("{}{}", lowered, suffix)
        }
    };

    if fixed == raw { None } else { Some(fixed) }
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NumericLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let oxc_ast::AstKind::NumericLiteral(lit) = node.kind() else {
            return;
        };
        let span = lit.span();
        let raw = &semantic.source_text()[span.start as usize..span.end as usize];
        if let Some(fixed) = canonical(raw) {
            let (line, col) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: format!(
                    "Invalid number literal casing: `{}` should be `{}`.",
                    raw, fixed
                ),
                severity: Severity::Error,
                span: None,
            });
        }
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

    // Regression for #5797: upper-case hex digits are a valid, common convention
    // (MIDI / protocol constants written to spec notation), so they must NOT be
    // flagged.
    #[test]
    fn allows_uppercase_hex_digits_issue_5797() {
        assert!(run("const rpn = [0x3D, 0x7F, 0xB0, 0xFF];").is_empty());
    }

    // Regression for #5980: lower-case hex digits are what Prettier / oxfmt emit,
    // so they must NOT be flagged — otherwise the rule fights the project
    // formatter and no source text can satisfy both.
    #[test]
    fn allows_lowercase_hex_digits_issue_5980() {
        let d = run("const buf = new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0xff, 0xff]);");
        assert!(d.is_empty());
    }

    // Genuinely mixed-case digits are emitted by no formatter; they are flagged
    // and normalised to upper-case (matching the Rust backend).
    #[test]
    fn flags_mixed_case_hex_digits() {
        let d = run("const x = 0xfF;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`0xfF` should be `0xFF`"));
    }

    // The prefix is normalised to lowercase; valid upper-case digits are kept.
    #[test]
    fn flags_uppercase_prefix_keeps_uppercase_digits() {
        let d = run("const x = 0XFF;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`0XFF` should be `0xFF`"));
    }

    // The prefix is normalised to lowercase; valid lower-case digits are kept.
    #[test]
    fn flags_uppercase_prefix_keeps_lowercase_digits() {
        let d = run("const x = 0Xff;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`0Xff` should be `0xff`"));
    }

    #[test]
    fn allows_canonical_uppercase_hex() {
        assert!(run("const x = 0xFF;").is_empty());
    }

    #[test]
    fn allows_canonical_lowercase_hex() {
        assert!(run("const x = 0xff;").is_empty());
    }

    // BigInt hex keeps the same digit-case tolerance and `n` suffix.
    #[test]
    fn allows_lowercase_bigint_hex() {
        assert!(run("const x = 0xffn;").is_empty());
    }

    #[test]
    fn flags_uppercase_exponent() {
        let d = run("const x = 1E3;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("1e3"));
    }

    #[test]
    fn allows_plain_numbers() {
        assert!(run("const x = 1234;").is_empty());
    }
}

//! number-literal-case — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

/// The canonical form: lowercase prefix/exponent and uppercase hex digits,
/// matching `unicorn/number-literal-case` (and oxlint): `0xFF`, `1e3`.
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
            let digits = &body[2..];
            format!("0x{}{}", digits.to_uppercase(), suffix)
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
                severity: Severity::Warning,
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

    // Regression for issue #5797: uppercase hex digits are canonical under
    // `unicorn/number-literal-case`, so MIDI/protocol constants written to match
    // spec notation (`0x3D`, `0x7F`) must NOT be flagged.
    #[test]
    fn allows_uppercase_hex_digits_issue_5797() {
        assert!(run("const rpn = [0x3D, 0x7F, 0xB0, 0xFF];").is_empty());
    }

    // Lowercase hex digits are non-canonical and fixed to uppercase.
    #[test]
    fn flags_lowercase_hex_digits() {
        let d = run("const buf = new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0xff, 0xff]);");
        assert_eq!(d.len(), 3);
        assert!(d[0].message.contains("`0x4b` should be `0x4B`"));
        assert!(d[1].message.contains("`0xff` should be `0xFF`"));
        assert!(d[2].message.contains("`0xff` should be `0xFF`"));
    }

    // The prefix stays lowercase even though the digits are uppercased.
    #[test]
    fn flags_uppercase_hex_prefix() {
        let d = run("const x = 0XFF;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`0XFF` should be `0xFF`"));
    }

    #[test]
    fn flags_uppercase_prefix_with_lowercase_digits() {
        let d = run("const x = 0Xff;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`0Xff` should be `0xFF`"));
    }

    #[test]
    fn allows_canonical_uppercase_hex() {
        assert!(run("const x = 0xFF;").is_empty());
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

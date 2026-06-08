//! text-encoding-identifier-case OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Known encoding identifiers and their canonical lowercase form.
const ENCODINGS: &[(&str, &str)] = &[
    ("UTF-8", "utf-8"),
    ("Utf-8", "utf-8"),
    ("UTF8", "utf8"),
    ("Utf8", "utf8"),
    ("ASCII", "ascii"),
    ("Ascii", "ascii"),
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StringLiteral(lit) = node.kind() else { return };
        let content = lit.value.as_str();

        for &(bad, good) in ENCODINGS {
            if content == bad {
                let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Prefer `'{good}'` over `'{bad}'`."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_uppercase_utf8_dash() {
        let d = run_on(r#"const enc = "UTF-8";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("utf-8"));
    }


    #[test]
    fn flags_mixed_case_utf8() {
        let d = run_on(r#"const enc = 'Utf-8';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("utf-8"));
    }


    #[test]
    fn flags_uppercase_ascii() {
        let d = run_on(r#"const enc = "ASCII";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("ascii"));
    }


    #[test]
    fn allows_lowercase_utf8() {
        assert!(run_on(r#"const enc = "utf-8";"#).is_empty());
    }


    #[test]
    fn allows_lowercase_ascii() {
        assert!(run_on(r#"const enc = 'ascii';"#).is_empty());
    }
}

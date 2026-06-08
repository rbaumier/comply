//! regex-prefer-predefined-assertion OXC backend — flag replaceable lookarounds in regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const WORD_BOUNDARY_PATTERNS: &[&str] = &[
    r"(?=\w)(?<=\W)",
    r"(?=\W)(?<=\w)",
    r"(?<=\w)(?=\W)",
    r"(?<=\W)(?=\w)",
];

const ANCHOR_PATTERNS: &[&str] = &["(?<=^)", "(?=$)"];

fn has_replaceable_assertion(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if !pattern.is_char_boundary(i) {
            i += 1;
            continue;
        }
        for pat in WORD_BOUNDARY_PATTERNS.iter().chain(ANCHOR_PATTERNS.iter()) {
            if pattern.get(i..i + pat.len()) == Some(*pat) {
                return true;
            }
        }
        i += 1;
    }
    false
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
        if !has_replaceable_assertion(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "This lookaround can be replaced with a predefined assertion like `\\b`, `^`, or `$`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_word_boundary_lookaround() {
        assert_eq!(run_on(r"const re = /(?=\w)(?<=\W)/;").len(), 1);
    }


    #[test]
    fn flags_start_anchor_lookaround() {
        assert_eq!(run_on(r"const re = /(?<=^)foo/;").len(), 1);
    }


    #[test]
    fn allows_normal_lookaround() {
        assert!(run_on(r"const re = /(?=foo)/;").is_empty());
    }


    #[test]
    fn ignores_lookaround_lookalike_in_tailwind_string() {
        let src = r#"const x = "group-[(?<=^)]:flex";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_lookaround_lookalike_in_url() {
        let src = r#"const u = "https://example.com/docs#(?=$)";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_lookaround_lookalike_in_scoped_import() {
        let src = r#"import X from "@scope/(?=\\w)(?<=\\W)";"#;
        assert!(run_on(src).is_empty());
    }
}

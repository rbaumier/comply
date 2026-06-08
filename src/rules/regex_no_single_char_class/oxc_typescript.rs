//! regex-no-single-char-class OXC backend — visits RegExpLiteral nodes only.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn find_single_char_classes(pattern: &str) -> Vec<String> {
    let mut hits = Vec::new();
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'['
            && bytes[i + 1] != b'^'
            && bytes[i + 1] != b'\\'
            && bytes[i + 1] != b']'
            && bytes[i + 2] == b']'
        {
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 == 0 {
                hits.push(pattern[i..i + 3].to_string());
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    hits
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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };
        let pattern = re.regex.pattern.text.as_str();
        for snippet in find_single_char_classes(pattern) {
            let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Unnecessary single-character class `{}` \u{2014} use the character directly (or escape it).",
                    snippet,
                ),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_single_char_class() {
        let diags = run_on(r#"const re = /[a]bc/;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("[a]"));
    }


    #[test]
    fn flags_dot_in_class() {
        let diags = run_on(r#"const re = /[.]foo/;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("[.]"));
    }


    #[test]
    fn allows_multi_char_class() {
        assert!(run_on(r#"const re = /[abc]/;"#).is_empty());
    }


    #[test]
    fn allows_negated_class() {
        assert!(run_on(r#"const re = /[^a]/;"#).is_empty());
    }


    #[test]
    fn allows_escape_in_class() {
        assert!(run_on(r#"const re = /[\d]/;"#).is_empty());
    }


    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[a]";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/[a]/path";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_empty_class_lookalike() {
        let src = r#"import X from "@scope/[a]-pkg";"#;
        assert!(run_on(src).is_empty());
    }
}

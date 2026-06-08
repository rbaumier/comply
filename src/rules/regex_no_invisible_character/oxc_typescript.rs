//! regex-no-invisible-character OXC backend — flag invisible Unicode codepoints
//! inside regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_invisible_char(c: char) -> bool {
    matches!(c,
        '\u{00AD}'
        | '\u{034F}'
        | '\u{061C}'
        | '\u{115F}'
        | '\u{1160}'
        | '\u{17B4}'
        | '\u{17B5}'
        | '\u{180E}'
        | '\u{2000}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{206F}'
        | '\u{FE00}'..='\u{FE0F}'
        | '\u{FEFF}'
        | '\u{FFF9}'..='\u{FFFB}'
    )
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
        if !pattern.chars().any(is_invisible_char) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Invisible Unicode character in regex \u{2014} use an explicit `\\u{...}` escape instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_zero_width_space() {
        assert_eq!(run_on("const re = /foo\u{200B}bar/;").len(), 1);
    }


    #[test]
    fn flags_soft_hyphen() {
        assert_eq!(run_on("const re = /test\u{00AD}word/;").len(), 1);
    }


    #[test]
    fn allows_clean_regex() {
        assert!(run_on("const re = /foo/;").is_empty());
    }


    #[test]
    fn allows_non_regex_line() {
        assert!(run_on("const x = 42;").is_empty());
    }


    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }


    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }


    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}

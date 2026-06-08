use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_multiple_spaces(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b' ' && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
            return true;
        }
        i += 1;
    }
    false
}

pub struct Check;

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
        if !has_multiple_spaces(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Multiple consecutive spaces in regex \u{2014} use a quantifier like ` {2}` instead.".into(),
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
    fn flags_double_space_in_literal() {
        assert_eq!(run_on("const re = /foo  bar/;").len(), 1);
    }


    #[test]
    fn flags_triple_space_in_literal() {
        assert_eq!(run_on("const re = /foo   bar/;").len(), 1);
    }


    #[test]
    fn allows_single_space() {
        assert!(run_on("const re = /foo bar/;").is_empty());
    }


    #[test]
    fn allows_quantifier() {
        assert!(run_on("const re = / {2}/;").is_empty());
    }


    #[test]
    fn ignores_tailwind_class_string_with_double_space() {
        // Runs of spaces inside a Tailwind class string are not a regex.
        let src = r#"const x = "px-4  py-2  text-sm";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_with_double_space_in_string() {
        let src = r#"const u = "https://example.com/a  b";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_path() {
        // `/` inside an import path used to be parsed as a regex by the
        // line scanner, producing false positives on any wide spacing
        // that appeared after it on the same line.
        let src = r#"import X from "@tanstack/react-query"; const s = "a  b";"#;
        assert!(run_on(src).is_empty());
    }
}

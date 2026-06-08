//! regex-no-obscure-range OXC backend — flag obscure character-class ranges.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const OBSCURE_RANGES: &[&str] = &["A-z", "a-Z", "0-z", "0-Z"];

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
        if !OBSCURE_RANGES.iter().any(|r| pattern.contains(r)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Character class range crosses ASCII groups (e.g. `[A-z]`) \u{2014} use `[A-Za-z]` instead.".into(),
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
    fn flags_a_to_z_upper_lower() {
        assert_eq!(run_on("const re = /[A-z]/;").len(), 1);
    }


    #[test]
    fn flags_zero_to_z() {
        assert_eq!(run_on("const re = /[0-z]/;").len(), 1);
    }


    #[test]
    fn allows_proper_range() {
        assert!(run_on("const re = /[A-Za-z]/;").is_empty());
    }


    #[test]
    fn allows_digit_range() {
        assert!(run_on("const re = /[0-9]/;").is_empty());
    }


    #[test]
    fn ignores_tailwind_class_string() {
        let src = r#"const x = "grid-cols-[A-z]";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/A-z/path";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@scope/0-z-pkg";"#;
        assert!(run_on(src).is_empty());
    }
}

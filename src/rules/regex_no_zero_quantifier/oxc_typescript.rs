//! regex-no-zero-quantifier oxc backend — detect `{0}` or `{0,0}` in regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_zero_quantifier(pattern: &str) -> bool {
    pattern.contains("{0}") || pattern.contains("{0,0}")
}

/// Extract the pattern from a regex literal's `raw` field (e.g. `/foo|bar/g` -> `foo|bar`).
fn extract_pattern(raw: &str) -> Option<&str> {
    let s = raw.strip_prefix('/')?;
    let last_slash = s.rfind('/')?;
    Some(&s[..last_slash])
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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };

        let Some(raw) = &re.raw else { return };
        let Some(pattern) = extract_pattern(raw.as_str()) else { return };

        if !has_zero_quantifier(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Zero quantifier `{0}` or `{0,0}` matches nothing \u{2014} remove or fix the quantifier.".into(),
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
    fn flags_zero_quantifier() {
        assert_eq!(run_on("const re = /a{0}/;").len(), 1);
    }


    #[test]
    fn flags_zero_zero_quantifier() {
        assert_eq!(run_on("const re = /a{0,0}/;").len(), 1);
    }


    #[test]
    fn allows_positive_quantifier() {
        assert!(run_on("const re = /a{1}/;").is_empty());
    }


    #[test]
    fn allows_range_quantifier() {
        assert!(run_on("const re = /a{0,1}/;").is_empty());
    }


    #[test]
    fn ignores_tailwind_class_with_zero_quantifier_lookalike() {
        let src = r#"const x = "grid-cols-[repeat(3,_minmax(0,_1fr))]{0}";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_with_zero_quantifier_lookalike() {
        let src = r#"const u = "https://example.com/path{0}";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_empty_string_with_quantifier_lookalike() {
        let src = r#"import x from "@scope/pkg/{0}";"#;
        assert!(run_on(src).is_empty());
    }
}

//! regex-no-slow-pattern OXC backend.
//!
//! Visits `RegExpLiteral` nodes only — string literals that happen to
//! look like regex are never flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Detects nested quantifiers like `(X+)+`, `(X*)*`, `(X+)*`, etc.
fn has_nested_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            let mut depth = 1;
            let mut j = i + 1;
            let mut inner_has_quantifier = false;
            let mut in_character_class = false;
            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => {
                        j += 1;
                    }
                    b'[' => in_character_class = true,
                    b']' => in_character_class = false,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'+' | b'*' if !in_character_class => inner_has_quantifier = true,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 && inner_has_quantifier && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'+' || next == b'*' {
                    return true;
                }
            }
            i = j + 1;
            continue;
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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };
        let pattern = re.regex.pattern.text.as_str();
        if !has_nested_quantifier(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Nested quantifier detected \u{2014} this pattern can cause catastrophic backtracking (ReDoS).".into(),
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
    fn flags_plus_plus() {
        assert_eq!(run_on(r#"const re = /(a+)+/;"#).len(), 1);
    }


    #[test]
    fn flags_star_star() {
        assert_eq!(run_on(r#"const re = /(.*)*$/;"#).len(), 1);
    }


    #[test]
    fn flags_plus_star() {
        assert_eq!(run_on(r#"const re = /(a+)*/;"#).len(), 1);
    }


    #[test]
    fn allows_single_quantifier() {
        assert!(run_on(r#"const re = /(a+)/;"#).is_empty());
    }


    #[test]
    fn allows_non_quantified_group() {
        assert!(run_on(r#"const re = /(abc)/;"#).is_empty());
    }


    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "grid-cols-[(a+)+_1fr]";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/(b+)+";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@scope/(pkg+)+";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_plus_literal_in_character_class() {
        assert!(run_on(r#"const re = /([a+])+/;"#).is_empty());
    }


    #[test]
    fn ignores_star_literal_in_character_class() {
        assert!(run_on(r#"const re = /([*])*/;"#).is_empty());
    }
}

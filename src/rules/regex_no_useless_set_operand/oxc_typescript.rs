//! regex-no-useless-set-operand OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::RegExpFlags;
use std::sync::Arc;

pub struct Check;

/// Detects useless operands in `v`-flag character class set operations.
fn has_useless_set_op(pattern: &str) -> bool {
    let complementary_pairs: &[(&str, &str)] = &[
        (r"\d", r"\w"),
        (r"\w", r"\W"),
        (r"\d", r"\D"),
        (r"\s", r"\S"),
    ];

    for &(a, b) in complementary_pairs {
        let intersection = format!("[{a}&&{b}]");
        let intersection_rev = format!("[{b}&&{a}]");
        let subtraction = format!("[{a}--{b}]");
        let subtraction_rev = format!("[{b}--{a}]");

        if pattern.contains(&intersection)
            || pattern.contains(&intersection_rev)
            || pattern.contains(&subtraction)
            || pattern.contains(&subtraction_rev)
        {
            return true;
        }
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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };

        if !re.regex.flags.contains(RegExpFlags::V) {
            return;
        }
        let pattern = re.regex.pattern.text.as_str();
        if !has_useless_set_op(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Useless operand in character class set operation.".into(),
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
    fn flags_subset_intersection() {
        assert_eq!(run_on(r#"const re = /[\d&&\w]/v;"#).len(), 1);
    }


    #[test]
    fn flags_complement_subtraction() {
        assert_eq!(run_on(r#"const re = /[\w--\W]/v;"#).len(), 1);
    }


    #[test]
    fn allows_non_v_flag() {
        assert!(run_on(r#"const re = /[\d]/g;"#).is_empty());
    }


    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/b";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_empty() {
        let src = r#"import "";"#;
        assert!(run_on(src).is_empty());
    }
}

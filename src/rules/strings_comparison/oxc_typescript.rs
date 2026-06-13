//! strings-comparison oxc backend — flag relational operators with string literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

/// Matches an ISO 8601 calendar date literal (`YYYY-MM-DD`, exactly 10 chars).
///
/// Such strings are designed so lexicographic order equals chronological order,
/// so a relational comparison against one is intentional (e.g. API version
/// negotiation) rather than an accidental string ordering.
fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 {
        return false;
    }
    bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

/// A flaggable string operand: a string literal that is not an ISO date.
fn is_flaggable_string_literal(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(lit) => !is_iso_date(&lit.value),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        if !matches!(
            bin.operator,
            BinaryOperator::LessThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::LessEqualThan
                | BinaryOperator::GreaterEqualThan
        ) {
            return;
        }

        if !is_flaggable_string_literal(&bin.left) && !is_flaggable_string_literal(&bin.right) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Relational comparison with string literal uses lexicographic order \u{2014} this is rarely the intent.".into(),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_relational_string_literal() {
        assert_eq!(run_on(r#"if (x >= "foo") {}"#).len(), 1);
    }

    #[test]
    fn allows_iso_date_version_comparison() {
        assert!(run_on(r#"if (version >= "2020-12-06") {}"#).is_empty());
    }

    #[test]
    fn allows_iso_date_less_than() {
        assert!(run_on(r#"if (version < "2025-07-05") {}"#).is_empty());
    }

    #[test]
    fn allows_both_iso_dates() {
        assert!(run_on(r#"if ("2020-12-06" < "2026-04-06") {}"#).is_empty());
    }

    #[test]
    fn flags_non_date_with_dashes() {
        assert_eq!(run_on(r#"if (v >= "20-12-2020") {}"#).len(), 1);
    }

    #[test]
    fn flags_datetime_string() {
        assert_eq!(run_on(r#"if (v >= "2020-12-06T00:00:00Z") {}"#).len(), 1);
    }
}

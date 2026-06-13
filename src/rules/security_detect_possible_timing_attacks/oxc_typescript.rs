//! security-detect-possible-timing-attacks oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

const SECRET_NAMES: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
    "apiKey",
    "api_key",
    "hash",
    "hashed_password",
    "hashedPassword",
    "signature",
    "sig",
    "csrfToken",
];

/// `null` / `undefined` / `""` — comparing a secret against one of these is an
/// absence check, not a byte-by-byte secret comparison, so it leaks nothing.
fn is_absence_sentinel(expr: &Expression) -> bool {
    match expr {
        Expression::NullLiteral(_) => true,
        Expression::Identifier(id) => id.name.as_str() == "undefined",
        Expression::StringLiteral(s) => s.value.is_empty(),
        _ => false,
    }
}

/// A timing attack only leaks a secret when both operands are runtime values
/// compared byte-by-byte. When one side is a string literal, its bytes are
/// already in the source — there is nothing to learn from timing the compare.
/// This is also where the `token` identifier of date/time format code lands:
/// `token === "yy"`, `token === "lastWeek"` are dispatch checks against format
/// codes, not secret comparisons.
fn is_string_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::StringLiteral(_))
}

fn name_is_secret(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => {
            let n = id.name.as_str().to_ascii_lowercase();
            SECRET_NAMES.iter().any(|s| s.to_ascii_lowercase() == n)
        }
        Expression::StaticMemberExpression(m) => {
            let n = m.property.name.as_str().to_ascii_lowercase();
            SECRET_NAMES.iter().any(|s| s.to_ascii_lowercase() == n)
        }
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
        let AstKind::BinaryExpression(bin) = node.kind() else {
            return;
        };
        if !matches!(
            bin.operator,
            BinaryOperator::Equality
                | BinaryOperator::StrictEquality
                | BinaryOperator::Inequality
                | BinaryOperator::StrictInequality
        ) {
            return;
        }
        if is_absence_sentinel(&bin.left) || is_absence_sentinel(&bin.right) {
            return;
        }
        if is_string_literal(&bin.left) || is_string_literal(&bin.right) {
            return;
        }
        if !name_is_secret(&bin.left) && !name_is_secret(&bin.right) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "String equality on a secret-looking identifier — short-circuit \
                      compare leaks bytes via timing. Use a constant-time compare \
                      (`crypto.timingSafeEqual`)."
                .into(),
            severity: Severity::Error,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_password_equality() {
        let src = r#"if (password === input) {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_token_member_equality() {
        let src = r#"if (user.token === provided) {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_secret_equality() {
        let src = r#"if (status === "active") {}"#;
        assert!(run(src).is_empty());
    }

    // Regression for #262: comparing a secret-looking field against an absence
    // sentinel checks presence, not a secret value.
    #[test]
    fn allows_secret_vs_null() {
        let src = r#"if (input.password !== null) {}"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_secret_vs_undefined() {
        assert!(run(r#"if (token === undefined) {}"#).is_empty());
    }

    #[test]
    fn allows_secret_vs_empty_string() {
        assert!(run(r#"if (password === "") {}"#).is_empty());
    }

    // Regression for #1914: `token` is a date-format token compared against a
    // hardcoded format code, not an auth secret. A literal operand cannot leak
    // a secret via timing, so these must not be flagged.
    #[test]
    fn allows_format_token_vs_literal() {
        let src = r#"const isTwoDigitYear = token === "yy";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_format_token_branch_vs_literal() {
        let src = r#"if (token === "lastWeek") { a(); } else if (token === "nextWeek") { b(); }"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // The genuine case stays flagged: two runtime values compared byte-by-byte.
    #[test]
    fn flags_token_vs_runtime_value() {
        let src = r#"if (token === userInput) {}"#;
        assert_eq!(run(src).len(), 1);
    }
}

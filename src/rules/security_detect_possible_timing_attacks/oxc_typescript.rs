//! security-detect-possible-timing-attacks oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, expression_is_or_resolves_to_literal, receiver_root_identifier,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

/// `hash` alone is excluded: a bare `.hash` property is the URL fragment
/// (`#section`) of a parsed route location (`a.hash`, `route.hash`,
/// `to.hash`), not a credential. A cryptographic digest in this codebase is
/// named explicitly (`hashedPassword`, `hashed_password`), and those stay.
const SECRET_NAMES: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
    "apiKey",
    "api_key",
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

/// True when `expr` is an enum/constant member access, e.g.
/// `AnnotationEditorType.SIGNATURE` or `Limits.MAX_LEN`: the chain is rooted on a
/// PascalCase type/enum identifier and the accessed property is SCREAMING_SNAKE_CASE
/// (the idiomatic JS/TS enum-member convention). That marks a compile-time constant
/// — its value is fixed at the call site, so, like a string literal, there is
/// nothing for timing to leak. A runtime secret read keeps a lowercase root: a
/// camelCase member (`user.token`, `req.body.password`) or a namespace accessor
/// (`process.env.SECRET`, whose property is also all-caps) stays flagged.
fn is_const_member(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(m) = expr else {
        return false;
    };
    is_screaming_snake(m.property.name.as_str())
        && receiver_root_identifier(expr).is_some_and(|root| {
            root.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        })
}

/// SCREAMING_SNAKE_CASE / all-uppercase constant name: at least one ASCII
/// uppercase letter, no ASCII lowercase letter, and a leading uppercase letter
/// (so a leading `_` or digit is rejected); interior `_` and digits are allowed.
fn is_screaming_snake(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_uppercase)
        && bytes
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}

/// True when both operands are member accesses rooted on the same object, e.g.
/// `data.password === data.confirmPassword` or `data.nested.confirm === data.password`.
/// Both sides are sibling fields of one user-supplied value being checked for a
/// match (a cross-field form-validation equality, as in a Zod `.refine`), so there
/// is no server-side secret to leak via timing — the attacker supplies both values.
/// Bare-identifier operands return `false`, keeping genuine `userInput === storedSecret`
/// comparisons flagged.
fn both_members_of_same_object(left: &Expression, right: &Expression) -> bool {
    let both_members = matches!(left, Expression::StaticMemberExpression(_))
        && matches!(right, Expression::StaticMemberExpression(_));
    if !both_members {
        return false;
    }
    match (
        receiver_root_identifier(left),
        receiver_root_identifier(right),
    ) {
        (Some(l), Some(r)) => l == r,
        _ => false,
    }
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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
        if is_const_member(&bin.left) || is_const_member(&bin.right) {
            return;
        }
        // An operand bound to a primitive literal (`const defaultApiKey = "9f7d…";
        // apiKey === defaultApiKey`) is, for timing purposes, identical to an
        // inline literal: its bytes are already in the source, so a leak reveals
        // nothing (e.g. ethers.js comparing against a public shared default API
        // key). A binding from a call or member access is a stored secret and is
        // not exempted here.
        if expression_is_or_resolves_to_literal(&bin.left, semantic)
            || expression_is_or_resolves_to_literal(&bin.right, semantic)
        {
            return;
        }
        if both_members_of_same_object(&bin.left, &bin.right) {
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

    // Regression for #3978: pdf.js compares a UI editor-mode against an enum
    // constant (`AnnotationEditorType.SIGNATURE` === 101). The SCREAMING_SNAKE
    // property is a compile-time constant — like a string literal, it leaks
    // nothing via timing — so enum-member dispatch must not be flagged. The
    // member only matched `SECRET_NAMES` because `SIGNATURE` lowercases to
    // `signature`.
    #[test]
    fn allows_enum_constant_member_equality() {
        assert!(run(r#"if (mode === AnnotationEditorType.SIGNATURE) {}"#).is_empty());
        assert!(run(r#"if (currentMode === AnnotationEditorType.SIGNATURE) {}"#).is_empty());
        assert!(run(r#"if (x === Foo.MAX_LEN) {}"#).is_empty());
    }

    // The enum constant on the left operand is exempted symmetrically.
    #[test]
    fn allows_enum_constant_member_on_left() {
        assert!(run(r#"if (AnnotationEditorType.SIGNATURE === mode) {}"#).is_empty());
    }

    // Narrowness guard: only the all-uppercase member convention is exempt. A
    // lowercase/camelCase member is a genuine runtime secret access and stays
    // flagged — `user.signature`/`user.token` are not enum constants.
    #[test]
    fn flags_lowercase_signature_member_equality() {
        assert_eq!(run(r#"if (obj.signature === sig) {}"#).len(), 1);
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

    // Regression for #3365: two sibling fields of the same user-supplied object are
    // a cross-field form-validation equality (e.g. a Zod `.refine`), not a secret
    // compared against attacker input. There is no server-side secret to leak.
    #[test]
    fn allows_password_vs_confirm_same_object() {
        let src = r#"baseSchema.refine((data) => data.password === data.confirmPassword);"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_nested_confirm_vs_password_same_root() {
        let src = r#"schema.refine((data) => data.nested.confirm === data.password);"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Different base objects keep firing: `req.body.password === user.passwordHash`
    // is the genuine attacker-input-vs-stored-secret comparison the rule targets.
    #[test]
    fn flags_secret_member_vs_other_object_member() {
        let src = r#"if (req.body.password === user.passwordHash) {}"#;
        assert_eq!(run(src).len(), 1);
    }

    // A bare-identifier secret compared against another value stays flagged even
    // when the other side is a member chain rooted elsewhere.
    #[test]
    fn flags_secret_identifier_vs_env_member() {
        let src = r#"if (token === process.env.SECRET) {}"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #3375: vuejs/router location.ts:187 and
    // defineColadaLoader.ts:748. A bare `.hash` property is the URL fragment of
    // a parsed route location, not a credential, so route-hash equality is not a
    // timing-attack target.
    #[test]
    fn allows_url_route_hash_comparison() {
        assert!(run(r#"if (a.hash === b.hash) {}"#).is_empty());
        assert!(run(r#"if (tracked.hash.v !== to.hash) {}"#).is_empty());
    }

    // Over-exemption guard: an explicitly-named cryptographic password hash
    // still flags — only the bare URL-fragment `hash` is exempt.
    #[test]
    fn flags_hashed_password_comparison() {
        assert_eq!(run(r#"if (hashedPassword === stored) {}"#).len(), 1);
    }

    // Regression for #3981: ethers.js compares a secret-looking identifier against
    // a module-level const bound to a string literal (the public shared default
    // API key). Its bytes are already in the source — one level of `const`
    // indirection from an inline literal — so timing leaks nothing.
    #[test]
    fn allows_secret_vs_const_bound_to_literal() {
        let src = r#"const defaultApiKey = "9f7d929b"; function f(apiKey) { return apiKey === defaultApiKey; }"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_member_secret_vs_const_bound_to_literal() {
        let src = r#"const defaultApiKey = "9f7d929b"; class P { check() { return this.apiKey === defaultApiKey; } }"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Narrowness guard: a const bound to a call or to `process.env` is a stored
    // secret, not an inline literal, so the comparison stays flagged.
    #[test]
    fn flags_secret_vs_const_bound_to_call() {
        let src = r#"const apiKey = getSecret(); if (apiKey === provided) {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_secret_vs_const_bound_to_env() {
        let src = r#"const secret = process.env.SECRET; if (token === secret) {}"#;
        assert_eq!(run(src).len(), 1);
    }
}

//! security-detect-possible-timing-attacks oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, callback_first_param_name, expression_is_or_resolves_to_literal,
    receiver_root_identifier,
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

/// An operand that provably cannot hold secret string bytes, so timing the
/// compare leaks nothing. Two cases:
/// - a PascalCase `Identifier`: almost always a class/constructor or namespace
///   reference (e.g. a NestJS DI token `FooService`), compared by reference
///   equality on a function object, not a secret byte sequence;
/// - a numeric literal: a number carries no secret bytes.
///
/// Scoped to leading-uppercase identifiers only: a lowercase identifier
/// (`password === userInput`, `token === name`) can bind a real secret string,
/// so it stays flagged — exempting it would be a security false negative.
fn is_non_secret_operand(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(id)
        if id.name.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
        || matches!(expr, Expression::NumericLiteral(_))
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

/// True when the comparison is structurally the returned predicate of an
/// `Array.prototype.filter` callback, AND at least one operand reads a field of
/// the iterated element (`sessions.filter(s => s.token !== revoked)`): the compare
/// decides whether each element stays in a rebuilt sub-collection the caller
/// already holds — list management, not a single-secret credential-equality gate
/// on an authentication path.
///
/// Why `filter` only: it is the sole array-iteration method that never
/// short-circuits — it evaluates the predicate for every element regardless of any
/// individual result — so it leaks neither which element matched (no early exit)
/// nor a guessable secret byte. Lookup / membership methods (`find` / `some` /
/// `every` / `findIndex` / …) stop at the first match, so a non-constant-time
/// compare in their predicate can leak via array-level timing; those stay flagged.
///
/// Soundness (this is a security rule):
/// - Only the callback's *returned predicate* position is exempt. Climbing up from
///   the comparison, the sole transparent parents are the boolean-combining
///   operators that still feed the callback's result (`&&`/`||`/`??`, `!`,
///   parentheses); the walk stops at the statement that yields the value. A
///   comparison in any other position — an `if` test, a call argument, a variable
///   initializer inside the callback body — is not the returned predicate and stays
///   flagged, so a genuine auth gate written inside a filter callback is not missed.
/// - The comparison must read the iterated element. A hoisted, element-independent
///   credential check (`filter(s => token === secret && s.active)`) does not, so it
///   stays flagged.
fn is_filter_element_predicate<'a>(
    bin_node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_span::GetSpan;
    let AstKind::BinaryExpression(bin) = bin_node.kind() else {
        return false;
    };
    let nodes = semantic.nodes();

    // Phase 1: confirm the comparison is the callback's returned predicate, and
    // capture the statement that yields it.
    let (stmt_node, require_expression_arrow) = {
        let mut cur = bin_node;
        loop {
            let parent = nodes.parent_node(cur.id());
            if parent.id() == cur.id() {
                return false;
            }
            match parent.kind() {
                AstKind::LogicalExpression(_)
                | AstKind::UnaryExpression(_)
                | AstKind::ParenthesizedExpression(_) => cur = parent,
                // Implicit-return arrow body (`s => s.token !== x`).
                AstKind::ExpressionStatement(_) => break (parent, true),
                // Explicit `return s.token !== x;` in a block-body callback.
                AstKind::ReturnStatement(_) => break (parent, false),
                _ => return false,
            }
        }
    };

    // Phase 2: resolve the nearest enclosing function. OXC can elide an inner
    // block-body function from the parent chain, so when a `FunctionBody` is
    // reached its owner is accepted only when their spans match; otherwise the
    // statement cannot be soundly attributed and the comparison stays flagged.
    let func = {
        let mut cur = stmt_node;
        loop {
            let parent = nodes.parent_node(cur.id());
            if parent.id() == cur.id() {
                return false;
            }
            match parent.kind() {
                AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => break parent,
                AstKind::FunctionBody(fb) => {
                    let owner = nodes.parent_node(parent.id());
                    let owns = match owner.kind() {
                        AstKind::ArrowFunctionExpression(a) => a.body.span() == fb.span(),
                        AstKind::Function(f) => {
                            f.body.as_ref().is_some_and(|b| b.span() == fb.span())
                        }
                        _ => false,
                    };
                    if owns {
                        break owner;
                    }
                    return false;
                }
                _ => cur = parent,
            }
        }
    };

    // Phase 3: the function must be the callback argument of a `.filter(…)` call.
    let func_span = match func.kind() {
        AstKind::ArrowFunctionExpression(arrow) => {
            if require_expression_arrow && !arrow.expression {
                return false;
            }
            arrow.span()
        }
        AstKind::Function(f) => {
            if require_expression_arrow {
                return false;
            }
            f.span()
        }
        _ => return false,
    };

    let call = nodes.parent_node(func.id());
    if call.id() == func.id() {
        return false;
    }
    let AstKind::CallExpression(call_expr) = call.kind() else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call_expr.callee else {
        return false;
    };
    if member.property.name.as_str() != "filter" {
        return false;
    }
    let is_callback_arg = call_expr
        .arguments
        .iter()
        .any(|arg| arg.as_expression().is_some_and(|e| e.span() == func_span));
    if !is_callback_arg {
        return false;
    }

    // The compared value must be a field of the iterated element — list-membership
    // management — not a hoisted, element-independent credential check. A
    // destructured first parameter yields no element name, so it stays flagged.
    let Some(param) = callback_first_param_name(call_expr) else {
        return false;
    };
    receiver_root_identifier(&bin.left).as_deref() == Some(param.as_str())
        || receiver_root_identifier(&bin.right).as_deref() == Some(param.as_str())
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
        if is_non_secret_operand(&bin.left) || is_non_secret_operand(&bin.right) {
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
        if is_filter_element_predicate(node, semantic) {
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

    // Regression for #3257: a NestJS DI token compared against a class/constructor
    // reference (`token === FooService`) is a reference-equality dispatch on a
    // function object, not a secret byte compare. The PascalCase operand is
    // exempt on either side.
    #[test]
    fn allows_di_token_vs_class_reference() {
        assert!(run(r#"if (token === FooService) {}"#).is_empty());
        assert!(run(r#"if (FooService === token) {}"#).is_empty());
    }

    // A numeric-literal operand carries no secret bytes, so the compare leaks
    // nothing via timing.
    #[test]
    fn allows_secret_vs_numeric_literal() {
        assert!(run(r#"if (token === 42) {}"#).is_empty());
    }

    // Security guard: a secret-name identifier compared against a lowercase
    // identifier stays flagged. `userInput` can be attacker-controlled — the
    // classic timing attack. Exempting lowercase identifiers would be a security
    // false negative.
    #[test]
    fn flags_secret_vs_lowercase_identifier() {
        assert_eq!(run(r#"if (password === userInput) {}"#).len(), 1);
    }

    // Security guard: the issue's lowercase second example (`token === name`)
    // stays flagged — `name` is a lowercase identifier that could bind a secret
    // string, so exempting it would be a security false negative.
    #[test]
    fn flags_di_token_vs_lowercase_name() {
        assert_eq!(run(r#"if (token === name) {}"#).len(), 1);
    }

    // Regression for #6200: better-auth rebuilds a stored session list by
    // dropping the current session (`internal-adapter.ts:391`). The comparison is
    // the returned predicate of an `Array.prototype.filter` callback — list
    // management over the caller's own sessions, not an authentication gate — so
    // it is not a timing-attack surface.
    #[test]
    fn allows_token_compare_in_filter_predicate() {
        let src = r#"list = list.filter((session) => session.expiresAt > now && session.token !== data.token);"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #6200: `session.ts:943`, the comparison is the whole arrow
    // body of a `.filter` callback.
    #[test]
    fn allows_token_compare_as_filter_arrow_body() {
        let src = r#"const others = activeSessions.filter((session) => session.token !== ctx.context.session.session.token);"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A block-body filter callback that returns the comparison is equally list
    // management.
    #[test]
    fn allows_token_compare_in_filter_block_body() {
        let src = r#"const kept = sessions.filter((s) => { return s.token !== data.token; });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Soundness guard: a credential-equality gate written *inside* a filter
    // callback body — but not as the returned predicate — is a genuine auth check
    // and stays flagged. Only the returned-predicate position is exempt.
    #[test]
    fn flags_auth_gate_inside_filter_callback_body() {
        let src = r#"list.filter((x) => { if (token === secret) { grant(); } return x.active; });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Soundness guard: an element-independent credential check hoisted into a
    // filter predicate (neither operand reads the iterated element `s`) is not
    // list management and stays flagged.
    #[test]
    fn flags_element_independent_compare_in_filter() {
        let src = r#"const kept = arr.filter((s) => token === secret && s.active);"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Security guard: lookup/membership methods short-circuit on the first match,
    // so a non-constant-time compare in their predicate can leak via array-level
    // timing. Only the non-short-circuiting `.filter` is exempt — `.find` stays
    // flagged.
    #[test]
    fn flags_token_lookup_in_find_callback() {
        let src = r#"const found = sessions.find((s) => s.token === requestToken);"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Negative space: `.map` is a transform, not the `.filter` list-rebuild
    // method, so a secret comparison in its callback is not exempted.
    #[test]
    fn flags_token_compare_in_map_callback() {
        let src = r#"const flags = users.map((u) => u.token === secret);"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Security guard: `.some` short-circuits on the first match, so it is not the
    // exempt `.filter` and stays flagged even with an element-rooted operand.
    #[test]
    fn flags_token_lookup_in_some_callback() {
        let src = r#"const present = tokens.some((s) => s.token === provided);"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Deliberate conservative choice: a destructured filter parameter yields no
    // element binding name, so element-rooting cannot be confirmed and the
    // comparison stays flagged (a residual false positive in the safe direction).
    #[test]
    fn flags_destructured_filter_param() {
        let src = r#"const kept = sessions.filter(({ token }) => token !== data.token);"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_or_resolves_to_literal};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryExpression, Expression};
use std::sync::Arc;

use super::helpers::{is_content_integrity_comparison, is_sensitive_identifier};

pub struct Check;

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
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        let AstKind::BinaryExpression(bin) = node.kind() else {
            return;
        };
        let op = bin.operator.as_str();
        if op != "==" && op != "!=" && op != "===" && op != "!==" {
            return;
        }
        if is_literal_expr(&bin.left) || is_literal_expr(&bin.right) {
            return;
        }
        // An operand bound to a primitive literal (`const k = "abc"; x === k`) is,
        // for timing purposes, identical to an inline literal: its bytes are
        // already present in the source, so a leak reveals nothing (e.g. ethers.js
        // comparing against a public shared default API key). A binding from a call
        // or member access is a stored secret and is not exempted here.
        if expression_is_or_resolves_to_literal(&bin.left, semantic)
            || expression_is_or_resolves_to_literal(&bin.right, semantic)
        {
            return;
        }
        // A `Symbol` is compared by reference identity (an O(1) pointer/id
        // check), not byte-by-byte, so a comparison where either operand is a
        // `Symbol()` / `Symbol.for(...)` value cannot leak timing. This covers
        // the capability-token idiom (`const secret = Symbol(); arg === secret`).
        if is_symbol_operand(&bin.left, semantic) || is_symbol_operand(&bin.right, semantic) {
            return;
        }
        // A function reference is compared by identity (a pointer/slot check),
        // not byte by byte, so it cannot leak a secret through timing. This
        // covers a sensitively-named operand bound to a function (a configurable
        // callback such as `issueRefreshToken`), where the `===`/`!==` asks "was
        // the default policy callback overridden?" rather than "do two secret
        // strings match?".
        if operand_is_function_reference(&bin.left, semantic)
            || operand_is_function_reference(&bin.right, semantic)
        {
            return;
        }
        // The class-defaults override idiom: `this.x !== this.#defaults.x`
        // compares a public property against the same-named property on a
        // private `#defaults` object, i.e. checks whether the configured value
        // still equals its built-in default (typically a callback). This is an
        // identity check against private internal state, not a remote-secret
        // comparison.
        if is_private_default_override_check(bin) {
            return;
        }
        let left_name = operand_name(&bin.left);
        let right_name = operand_name(&bin.right);
        // A content-integrity / checksum comparison (e.g. a downloaded file's
        // SHA-256 digest against its expected value) compares public
        // fingerprints, not secrets, so it is not a timing-attack target.
        if is_content_integrity_comparison(left_name.as_deref(), right_name.as_deref()) {
            return;
        }
        let left_hit = left_name.as_deref().is_some_and(is_sensitive_identifier);
        let right_hit = right_name.as_deref().is_some_and(is_sensitive_identifier);
        if !left_hit && !right_hit {
            return;
        }
        // Skip confirmation-style comparisons where both operands come from
        // the same object (e.g. `data.password === data.confirmPassword`).
        if both_from_same_object(bin) {
            return;
        }
        // Skip confirmation-pattern comparisons: both operands are sensitive
        // identifiers and one contains a confirmation prefix/suffix.
        if left_hit && right_hit && is_identifier(&bin.left) && is_identifier(&bin.right)
            && let (Some(l), Some(r)) = (&left_name, &right_name) {
                let combined = format!("{l}{r}");
                let lower = combined.to_ascii_lowercase();
                if lower.contains("confirm")
                    || lower.contains("repeat")
                    || lower.contains("retype")
                    || lower.contains("verify")
                {
                    return;
                }
            }
        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Direct comparison of a security-sensitive value \u{2014} use a constant-time comparison (`crypto.timingSafeEqual`).".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

fn is_literal_expr(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::NullLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
    ) || is_undefined(expr)
}

fn is_undefined(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name == "undefined")
}

fn is_identifier(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(_))
}

fn operand_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.to_string()),
        _ => None,
    }
}

fn member_object_text(expr: &Expression) -> Option<String> {
    match expr {
        Expression::StaticMemberExpression(member) => {
            expr_text(&member.object)
        }
        _ => None,
    }
}

fn expr_text(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(member) => {
            let obj = expr_text(&member.object)?;
            Some(format!("{}.{}", obj, member.property.name))
        }
        _ => None,
    }
}

/// True when `expr` is a `Symbol(...)` or `Symbol.for(...)` call expression.
/// These produce JS `Symbol` values, which are compared by reference identity
/// rather than byte content.
fn is_symbol_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    match &call.callee {
        Expression::Identifier(id) => id.name == "Symbol",
        Expression::StaticMemberExpression(member) => {
            matches!(&member.object, Expression::Identifier(id) if id.name == "Symbol")
                && member.property.name == "for"
        }
        _ => false,
    }
}

/// True when `expr` is provably a `Symbol` value: either an inline
/// `Symbol(...)` / `Symbol.for(...)` call, or an identifier whose binding is a
/// `const`/`let` declarator initialised from such a call. Symbols are compared
/// by reference identity, so they are immune to timing attacks regardless of
/// the equality operator used.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// inspects the `VariableDeclarator` initializer. A binding without a
/// `Symbol(...)` initializer (a stored secret string, buffer, or token) does
/// not match and stays flagged.
fn is_symbol_operand(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_ast::AstKind;

    if is_symbol_call(expr) {
        return true;
    }
    let Expression::Identifier(ident) = expr else {
        return false;
    };
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return decl.init.as_ref().is_some_and(is_symbol_call);
        }
    }
    false
}

/// True when `expr` is provably a reference to a function rather than a secret
/// string/buffer value. A function is compared by reference identity, so an
/// equality check on one cannot leak a secret through timing.
///
/// Only the plain-identifier case is resolved: the binding is followed via
/// `reference_id` → symbol → declaration node, and matches when the declaration
/// is a `FunctionDeclaration`, or a `const`/`let` `VariableDeclarator` whose
/// initializer is an arrow function or function expression. A binding that does
/// not resolve, or resolves to a non-function value (a stored secret string,
/// buffer, or token), does not match and stays flagged.
fn operand_is_function_reference(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_ast::AstKind;

    let Expression::Identifier(ident) = expr else {
        return false;
    };
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    // A `function foo() {}` binding declares the symbol directly on the
    // `Function` node, so only the declaration node itself counts — never an
    // ancestor, which would be the *enclosing* function of an unrelated binding.
    if let AstKind::Function(func) = nodes.kind(decl_node_id)
        && func.is_declaration()
    {
        return true;
    }
    // A `const foo = () => {}` binding declares the symbol on the binding
    // identifier nested under a `VariableDeclarator`; walk up to that declarator
    // and inspect its initializer, stopping at the first one found so the search
    // cannot escape into an enclosing scope.
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return matches!(
                decl.init.as_ref(),
                Some(Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_))
            );
        }
    }
    false
}

/// True when `bin` compares a value against the same-named property on a private
/// field — the `this.x !== this.#defaults.x` override-check idiom. One operand
/// reaches a property through a private-field expression (`this.#defaults.x`)
/// and the other reads a property of the same trailing name (`this.x`), so the
/// comparison asks "is the configured value still the built-in default?" rather
/// than matching two secret strings.
fn is_private_default_override_check(bin: &BinaryExpression) -> bool {
    let private_then_plain = |private_side: &Expression, plain_side: &Expression| -> bool {
        matches!(
            (
                member_property_via_private_field(private_side),
                operand_name(plain_side),
            ),
            (Some(a), Some(b)) if a == b
        )
    };
    private_then_plain(&bin.left, &bin.right) || private_then_plain(&bin.right, &bin.left)
}

/// If `expr` is a `<object>.<property>` access whose receiver chain reaches a
/// private-field access (`this.#defaults.foo`, `this.#defaults.bar.foo`),
/// returns the trailing property name; otherwise `None`.
fn member_property_via_private_field(expr: &Expression) -> Option<String> {
    let Expression::StaticMemberExpression(member) = expr else {
        return None;
    };
    if object_reaches_private_field(&member.object) {
        Some(member.property.name.to_string())
    } else {
        None
    }
}

/// True when `expr` is, or accesses a property through, a private-field
/// expression (`this.#x`, `this.#x.y`).
fn object_reaches_private_field(expr: &Expression) -> bool {
    match expr {
        Expression::PrivateFieldExpression(_) => true,
        Expression::StaticMemberExpression(member) => {
            object_reaches_private_field(&member.object)
        }
        _ => false,
    }
}

fn both_from_same_object(bin: &BinaryExpression) -> bool {
    let left_obj = member_object_text(&bin.left);
    let right_obj = member_object_text(&bin.right);
    matches!((left_obj, right_obj), (Some(a), Some(b)) if a == b)
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
    fn flags_password_comparison() {
        assert_eq!(run_on("if (password === input) {}").len(), 1);
    }

    #[test]
    fn flags_auth_token_comparison() {
        assert_eq!(run_on("if (authToken == expectedAuthToken) {}").len(), 1);
    }

    #[test]
    fn flags_member_expression_password() {
        assert_eq!(run_on("if (user.password === input) {}").len(), 1);
    }

    #[test]
    fn flags_nested_member_expression_password() {
        assert_eq!(
            run_on("if (req.body.password === user.passwordHash) {}").len(),
            1
        );
    }

    #[test]
    fn flags_api_key_pascal_case() {
        assert_eq!(
            run_on("if (req.headers.apiKey === process.env.API_KEY) {}").len(),
            1
        );
    }

    #[test]
    fn allows_non_sensitive_comparison() {
        assert!(run_on("if (name === other) {}").is_empty());
    }

    #[test]
    fn allows_token_type_lexer() {
        assert!(run_on("if (tokenType === TokenType.Identifier) {}").is_empty());
    }

    #[test]
    fn allows_hash_map_size() {
        assert!(run_on("if (hashMapSize === 0) {}").is_empty());
    }

    /// `token` / `signature` without a secret indicator are non-security
    /// role words (lexer tokens, LSP signatures), not credentials.
    #[test]
    fn allows_comment_token_and_lsp_signature() {
        assert!(run_on("if (commentToken !== currentCommentToken) {}").is_empty());
        assert!(run_on("if (oldLspSig !== lspSignature) {}").is_empty());
    }

    #[test]
    fn allows_string_literal_with_sensitive_word() {
        assert!(run_on(r#"if (node.kind() !== "index_signature") {}"#).is_empty());
    }

    #[test]
    fn allows_no_comparison() {
        assert!(run_on("const password = getPassword();").is_empty());
    }

    #[test]
    fn allows_null_check() {
        assert!(run_on("if (token === null) {}").is_empty());
    }

    #[test]
    fn allows_undefined_check() {
        assert!(run_on("if (password !== undefined) {}").is_empty());
    }

    #[test]
    fn allows_empty_string_check() {
        assert!(run_on(r#"if (secret === "") {}"#).is_empty());
    }

    #[test]
    fn allows_boolean_check() {
        assert!(run_on("if (token === false) {}").is_empty());
    }

    /// A `Symbol()` capability token (prisma's `Skip` / `nullTypes` idiom) is
    /// reference-compared, not byte-compared, so it is immune to timing
    /// attacks even though the identifier ends with `secret`.
    #[test]
    fn allows_symbol_capability_token() {
        assert!(run_on("const secret = Symbol(); function f(arg) { if (arg === secret) {} }").is_empty());
        assert!(run_on("const secret = Symbol(); function f(param) { if (param !== secret) {} }").is_empty());
    }

    #[test]
    fn allows_symbol_for_capability_token() {
        assert!(
            run_on("const secret = Symbol.for('x'); function f(arg) { if (arg === secret) {} }")
                .is_empty()
        );
    }

    #[test]
    fn allows_inline_symbol_comparison() {
        assert!(run_on("if (arg === Symbol.for('skip')) {}").is_empty());
    }

    /// A genuine secret bound to a string (not a `Symbol`) must still flag.
    #[test]
    fn flags_secret_string_comparison() {
        assert_eq!(
            run_on("const secret = getSecret(); function f(arg) { if (arg === secret) {} }").len(),
            1
        );
    }

    /// ethers.js provider-ankr.ts:136,157 — `defaultApiKey` is a module-level
    /// const bound to a string literal (a public shared default key whose bytes
    /// are already in the source). Comparing against it is identical, for timing
    /// purposes, to comparing against an inline literal, so it must not flag.
    #[test]
    fn allows_const_bound_to_string_literal() {
        assert!(
            run_on(
                r#"const defaultApiKey = "9f7d929b018cdffb338517efa06f58359e86ff1ffd350bc889738523659e7972"; function f(apiKey) { return apiKey === defaultApiKey; }"#
            )
            .is_empty()
        );
    }

    /// A const bound to a numeric literal is likewise a public inline value.
    #[test]
    fn allows_const_bound_to_numeric_literal() {
        assert!(
            run_on("const apiKey = 12345; function f(token) { if (token === apiKey) {} }")
                .is_empty()
        );
    }

    /// Over-exemption guard: a const bound to a non-literal expression
    /// (`process.env.KEY`, a `"a" + x` concatenation) is a stored secret, not an
    /// inline literal, and must still flag.
    #[test]
    fn flags_const_bound_to_non_literal() {
        assert_eq!(
            run_on("const apiKey = process.env.API_KEY; function f(token) { if (token === apiKey) {} }").len(),
            1
        );
        assert_eq!(
            run_on(r#"const secret = "a" + getSalt(); function f(token) { if (token === secret) {} }"#).len(),
            1
        );
    }

    /// prisma/fetch-engine downloadZip.ts:131,135 — comparing a downloaded
    /// file's computed SHA-256 digest against its expected checksum is a
    /// content-integrity check on public fingerprints, not a secret check.
    #[test]
    fn allows_sha256_integrity_comparison() {
        assert!(
            run_on("if (zippedSha256 !== null && zippedSha256 !== zippedHash) {}").is_empty()
        );
        assert!(run_on("if (sha256 !== null && sha256 !== hash) {}").is_empty());
    }

    /// Over-exemption guard: a real password / token comparison carries no
    /// integrity indicator and must still flag.
    #[test]
    fn flags_password_despite_integrity_exemption() {
        assert_eq!(run_on("if (password === input) {}").len(), 1);
        assert_eq!(run_on("if (authToken !== expectedAuthToken) {}").len(), 1);
    }

    /// vuejs/router location.ts:187 + defineColadaLoader.ts:748 — a bare
    /// `.hash` property is the URL fragment (`#section`) of a parsed route
    /// location, not a cryptographic digest, so comparing route hashes for
    /// equality is not a timing-attack target.
    #[test]
    fn allows_url_route_hash_comparison() {
        assert!(run_on("if (a.hash === b.hash) {}").is_empty());
        assert!(run_on("if (tracked.hash.v !== to.hash) {}").is_empty());
        assert!(run_on("if (location.hash === '#footer') {}").is_empty());
    }

    /// panva/node-oidc-provider configuration.js:173 — `issueRefreshToken` is a
    /// configurable policy callback (a function). The `!==` against the same
    /// property on a private `#defaults` object asks whether the user overrode
    /// the default callback, an identity check on function references, not a
    /// secret-string comparison.
    #[test]
    fn allows_private_defaults_override_check() {
        assert!(
            run_on(
                "class C { #defaults = getDefaults(); f() { return this.issueRefreshToken !== this.#defaults.issueRefreshToken; } }"
            )
            .is_empty()
        );
        // Operands in the other order.
        assert!(
            run_on(
                "class C { #defaults = getDefaults(); f() { return this.#defaults.accessToken === this.accessToken; } }"
            )
            .is_empty()
        );
    }

    /// A sensitively-named operand bound to a function reference is compared by
    /// identity, not byte content, so it cannot leak a secret through timing.
    #[test]
    fn allows_function_reference_comparison() {
        assert!(
            run_on("function issueToken() {} function f(x) { if (x === issueToken) {} }")
                .is_empty()
        );
        assert!(
            run_on("const issueRefreshToken = () => true; function f(x) { return x !== issueRefreshToken; }")
                .is_empty()
        );
        assert!(
            run_on("const authToken = function () {}; function f(x) { if (x === authToken) {} }")
                .is_empty()
        );
    }

    /// Over-exemption guard: a sensitive operand bound to a non-function value
    /// (a stored secret string) must still flag, and a same-named comparison
    /// that does NOT reach through a private field is not the override idiom.
    #[test]
    fn flags_secret_string_not_function_reference() {
        assert_eq!(
            run_on("const authToken = getSecret(); function f(x) { if (x === authToken) {} }").len(),
            1
        );
        // No private-field defaults object: a plain `this.authToken === other.authToken`
        // is a genuine secret comparison and is not the override idiom.
        assert_eq!(
            run_on("class C { f(other) { return this.authToken === other.authToken; } }").len(),
            1
        );
    }

    /// Over-exemption guard: a qualified cryptographic hash still flags — the
    /// URL-fragment exemption is scoped to bare/routing `hash`.
    #[test]
    fn flags_password_hash_comparison() {
        assert_eq!(run_on("if (passwordHash === storedHash) {}").len(), 1);
        assert_eq!(run_on("if (user.password_hash === expectedHash) {}").len(), 1);
    }
}

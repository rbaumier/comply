//! no-timing-attack Rust backend.
//!
//! Walks `binary_expression` nodes whose operator is `==` / `!=` and
//! flags the comparison if either operand refers to an identifier whose
//! normalized name is sensitive (see `is_sensitive_identifier`). Operands
//! that are string literals, call expressions, or any other shape are
//! ignored, so a string like `"index_signature"` is never inspected.
//!
//! A comparison is also exempt when it cannot leak a secret through timing:
//! when it is a scalar-integer or boolean comparison (a single constant-time
//! instruction), or when either operand is a string / char literal (a public
//! compile-time constant baked into the binary, not a runtime secret).
//!
//! Test code is out of scope — it is no attack surface. That covers the gates
//! the file itself carries (`#[cfg(test)]`, `#[test]`) and the ones it cannot:
//! a Cargo `tests/` target and a module gated by its parent's
//! `#[cfg(test)] mod …;` (`rust_helpers::is_test_only_rust_file`).

use crate::diagnostic::{Diagnostic, Severity};

use super::helpers::{is_content_integrity_comparison, is_sensitive_identifier};

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op) = node.child_by_field_name("operator") else { return };
    let op_text = op.utf8_text(source).unwrap_or("");
    if op_text != "==" && op_text != "!=" {
        return;
    }
    if is_in_partial_eq_eq_method(node, source) {
        return;
    }
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let left_name = operand_name(left, source);
    let right_name = operand_name(right, source);
    // A content-integrity / checksum comparison (e.g. a file's SHA-256 digest
    // against its expected value) compares public fingerprints, not secrets,
    // so it is not a timing-attack target.
    if is_content_integrity_comparison(left_name, right_name) {
        return;
    }
    let left_hit = left_name.is_some_and(is_sensitive_identifier);
    let right_hit = right_name.is_some_and(is_sensitive_identifier);
    if !left_hit && !right_hit {
        return;
    }
    // Test code is no attack surface: it runs under `cargo test` and never ships.
    // The gate can sit on the comparison's own ancestors, on the `mod`
    // declaration in a parent file, or nowhere at all (a Cargo `tests/` target).
    // Both lookups are far costlier than the name match, so they run last.
    if crate::rules::rust_helpers::is_in_test_context(node, source)
        || crate::rules::rust_helpers::is_test_only_rust_file(ctx.path, ctx.project)
    {
        return;
    }
    // A scalar-integer comparison is a single constant-time machine instruction,
    // not a byte-by-byte one, so it cannot leak a secret through timing. When
    // either operand is provably an integer (an integer literal, or a local bound
    // to a count / dimension), the sensitively-named operand is a numeric count
    // (e.g. `pin` = the length of an HMM initial-state vector), not a credential.
    if comparison_is_scalar_integer(left, right, source) {
        return;
    }
    // A boolean comparison holds one bit per side, which the CPU compares in a
    // single constant-time instruction. When either operand is provably a `bool`,
    // the sensitively-named operand states a property of the secret
    // (`password.is_some()`), never the secret itself.
    if comparison_is_boolean(left, right, source) {
        return;
    }
    // A string / char literal operand is a public compile-time constant baked
    // into the binary, not a runtime secret. Comparing a sensitively-named value
    // against a literal (e.g. `password == "iamrds"`, a mode-selecting sentinel)
    // has no timing-attack surface: leaking that the value matches a known public
    // constant reveals nothing. Sibling to the integer-literal exemption above.
    if comparison_has_string_or_char_literal(left, right) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-timing-attack".into(),
        message: "Direct comparison of a security-sensitive value \u{2014} use a constant-time comparison (`constant_time_eq::constant_time_eq`, `subtle::ConstantTimeEq`).".into(),
        severity: Severity::Error,
        span: None,
    });
}

fn operand_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok(),
        "field_expression" => node
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok()),
        _ => None,
    }
}

/// True when the comparison is provably between scalar integers — either operand
/// is an integer literal, or a bare identifier whose local binding proves it is
/// an integer. Rust is statically typed, so one provably-integer operand means
/// both sides are integers.
fn comparison_is_scalar_integer(
    left: tree_sitter::Node,
    right: tree_sitter::Node,
    source: &[u8],
) -> bool {
    operand_is_scalar_integer(left, source) || operand_is_scalar_integer(right, source)
}

fn operand_is_scalar_integer(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "integer_literal" => true,
        "identifier" => node.utf8_text(source).is_ok_and(|name| {
            crate::rules::rust_helpers::local_binding_is_integer(node, name, source)
        }),
        _ => false,
    }
}

/// True when the comparison is provably between booleans — either operand is
/// e.g. a `true` / `false` literal, a logical negation, a nested comparison, a
/// `matches!`, a bool-returning call, or a bool-typed binding; the full shape
/// list lives in `rust_helpers::operand_is_bool`. Rust is statically typed, so
/// one provably-boolean operand means both sides are `bool`.
fn comparison_is_boolean(left: tree_sitter::Node, right: tree_sitter::Node, source: &[u8]) -> bool {
    crate::rules::rust_helpers::operand_is_bool(left, source)
        || crate::rules::rust_helpers::operand_is_bool(right, source)
}

/// True when either operand is a string, raw-string, or char literal — a public
/// compile-time constant, so the comparison holds no secret to leak. Byte strings
/// (`b"..."`) and byte chars (`b'x'`) parse as `string_literal` / `char_literal`,
/// so they are covered too.
fn comparison_has_string_or_char_literal(
    left: tree_sitter::Node,
    right: tree_sitter::Node,
) -> bool {
    operand_is_string_or_char_literal(left) || operand_is_string_or_char_literal(right)
}

fn operand_is_string_or_char_literal(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "string_literal" | "raw_string_literal" | "char_literal"
    )
}

/// True if `node` sits inside the `eq` method of an `impl PartialEq for T`.
///
/// A `self.hash == other.hash` fast-path inside `PartialEq::eq` compares two
/// fields of the same `&Self` value — a structural-hash short-circuit, not a
/// secret check. There is no attacker-controlled input vs. stored-secret
/// asymmetry, so the timing-attack premise does not apply and the comparison is
/// exempt.
///
/// Walks up to the nearest enclosing `function_item`; the exemption applies only
/// when that method is named `eq` and its enclosing `impl_item` implements
/// `PartialEq` (bare or generic `PartialEq<Rhs>`, optionally path-qualified).
fn is_in_partial_eq_eq_method(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "function_item" {
            let is_eq = ancestor
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                == Some("eq");
            return is_eq && enclosing_impl_is_partial_eq(ancestor, source);
        }
        current = ancestor.parent();
    }
    false
}

/// True if the nearest `impl_item` enclosing `node` implements `PartialEq`.
fn enclosing_impl_is_partial_eq(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            let Some(trait_node) = ancestor.child_by_field_name("trait") else {
                return false;
            };
            let trait_text = trait_node.utf8_text(source).unwrap_or("");
            // Strip any `<Rhs>` generic args, then the trailing path segment, so
            // `PartialEq`, `PartialEq<Self>`, and `core::cmp::PartialEq` all match.
            let bare = trait_text
                .split('<')
                .next()
                .unwrap_or(trait_text)
                .rsplit("::")
                .next()
                .unwrap_or(trait_text)
                .trim();
            return bare == "PartialEq";
        }
        current = ancestor.parent();
    }
    false
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    /// A comparison the rule flags on its own, so a multi-file test reads as a
    /// question about the file's role rather than about the comparison.
    const PASSWORD_COMPARISON: &str =
        "pub fn same(password: &str, other: &str) -> bool { password == other }";

    #[test]
    fn flags_password_comparison() {
        let src = "fn f(password: &str, input: &str) -> bool { password == input }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_auth_token_comparison() {
        let src = "fn f() -> bool { auth_token == expected_auth_token }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_field_expression_password() {
        // `user.password_hash == input` — left is a field_expression.
        let src = "fn f(user: &User, input: &str) -> bool { user.password_hash == input }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_hash_comparison() {
        let src = "fn f() -> bool { password_hash != received_hash }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// quinn/tests/many_connections.rs:174 (#8266) — a CRC32 check of a public
    /// payload. `encoded_hash` and `actual_hash` name the two roles of the
    /// comparison and nothing else: neither operand is a credential.
    #[test]
    fn does_not_flag_crc32_role_named_hash_comparison() {
        let src = r#"
fn hash_correct(data: &[u8], crc: &Crc<u32>) -> bool {
    let encoded_hash = ((data[0] as u32) << 24) | data[3] as u32;
    let actual_hash = crc.checksum(&data[4..]);
    encoded_hash == actual_hash
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_api_key_snake_case() {
        // Normalized form is "apikey" — ends_with "apikey".
        let src = "fn f() -> bool { supplied_api_key == known_api_key }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_non_sensitive_comparison() {
        let src = "fn f() -> bool { name == other }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_token_type_lexer() {
        // `token_type` normalizes to "tokentype" — ends with "type",
        // not sensitive.
        let src = "fn f() -> bool { token_type == other_type }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_hashmap_size() {
        let src = "fn f() -> bool { hashmap_size == 0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_signature_bytes_count() {
        let src = "fn f() -> bool { signature_bytes != 64 }";
        assert!(run_on(src).is_empty());
    }

    /// The exact FP observed during the walkthrough: a string literal
    /// `"index_signature"` (tree-sitter node kind) compared via `!=`.
    #[test]
    fn does_not_flag_string_literal_containing_signature() {
        let src = r#"
fn check(member: tree_sitter::Node) {
    if member.kind() != "index_signature" {
        return;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_call_expression_operand() {
        // `member.kind() != "foo"` — left is a call_expression, right is
        // a string literal. Neither is inspected.
        let src = r#"
fn check(member: tree_sitter::Node) {
    let _ = member.kind() != "password";
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// helix-core/src/comment.rs:63 — `token` is a comment-syntax marker
    /// (`//`, `#`, …), not an auth token.
    #[test]
    fn does_not_flag_comment_syntax_token() {
        let src = "fn f(fragment: &str, token: &str) -> bool { fragment != token }";
        assert!(run_on(src).is_empty());
    }

    /// helix-term/src/commands.rs:5305 — `current_comment_token` is the
    /// active comment prefix in the editor.
    #[test]
    fn does_not_flag_current_comment_token() {
        let src =
            "fn f(token: &str, current_comment_token: Option<&str>) -> bool { Some(token) == current_comment_token }";
        assert!(run_on(src).is_empty());
    }

    /// helix-term/src/handlers/signature_help.rs:253 — `lsp_signature` is
    /// an LSP function-call signature, not a digital signature.
    #[test]
    fn does_not_flag_lsp_signature() {
        let src = "fn f(old_lsp_sig: &Sig, lsp_signature: &Sig) -> bool { old_lsp_sig != lsp_signature }";
        assert!(run_on(src).is_empty());
    }

    /// bevy_platform/src/hash.rs:95 — a structural-hash fast-path inside
    /// `PartialEq::eq`; both operands are fields of `&Self`, no secret.
    #[test]
    fn does_not_flag_hash_fastpath_in_partial_eq() {
        let src = r#"
impl<V: PartialEq, H> PartialEq for Hashed<V, H> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.value.eq(&other.value)
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// bevy_diagnostic/src/diagnostic.rs:97 — same shape on `DiagnosticPath`.
    #[test]
    fn does_not_flag_hash_in_partial_eq_diagnostic_path() {
        let src = r#"
impl PartialEq for DiagnosticPath {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.path == other.path
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Generic `PartialEq<Rhs>` is still the equality trait — exempt.
    #[test]
    fn does_not_flag_hash_in_generic_partial_eq() {
        let src = r#"
impl PartialEq<Other> for Thing {
    fn eq(&self, other: &Other) -> bool {
        self.hash == other.hash
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Negative-space guard: a genuine secret comparison inside `eq` of a
    /// non-equality trait is NOT exempt — only `PartialEq::eq` is.
    #[test]
    fn flags_password_in_non_partial_eq_trait_eq_method() {
        let src = r#"
impl MyTrait for Thing {
    fn eq(&self, other: &Self) -> bool {
        self.password == other.password
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// Negative-space guard: a credential comparison in an inherent method (no
    /// trait) is NOT exempt — the exemption is scoped to `PartialEq::eq`.
    #[test]
    fn flags_secret_in_inherent_eq_method() {
        let src = r#"
impl Thing {
    fn eq(&self, other: &Self) -> bool {
        self.password_hash == other.password_hash
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// Negative-space guard: a secret comparison in a non-`eq` method of a
    /// `PartialEq` impl is NOT exempt — only the `eq` method short-circuit is.
    #[test]
    fn flags_password_in_partial_eq_non_eq_method() {
        let src = r#"
impl PartialEq for Thing {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
    fn check(&self, input: &str) -> bool {
        self.password == input
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// #3375: a bare `hash` is overloaded — a URL fragment / structural hash,
    /// not a credential — so a comparison of bare `hash` operands is exempt
    /// even outside a `PartialEq` impl.
    #[test]
    fn allows_bare_hash_comparison() {
        let src = "fn process(info: &Info, new_hash: u64) -> bool { info.hash == new_hash }";
        assert!(run_on(src).is_empty());
    }

    /// Over-exemption guard: a qualified cryptographic hash carries a crypto
    /// qualifier and stays flagged — only the bare `hash` is exempt.
    #[test]
    fn flags_qualified_crypto_hash_comparison() {
        let src = "fn f(password_hash: &str, expected_hash: &str) -> bool { password_hash == expected_hash }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// A SHA-256 content-integrity check compares public fingerprints, so it
    /// is exempt even though one operand ends with `hash` (#3352).
    #[test]
    fn allows_sha256_integrity_comparison() {
        let src = "fn verify(sha256: &str, hash: &str) -> bool { sha256 != hash }";
        assert!(run_on(src).is_empty());
    }

    /// Over-exemption guard: a real credential comparison carries no integrity
    /// indicator and must still flag.
    #[test]
    fn flags_password_despite_integrity_exemption() {
        let src = "fn f(password: &str, input: &str) -> bool { password == input }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// rust-bio/src/stats/hmm/mod.rs:750 (#6052) — `pin` is a `usize` dimension
    /// count (the length of the HMM initial-state probability vector), bound from
    /// `initial.dim()` and compared against other dimension counts. A scalar
    /// integer comparison is constant-time, not a timing-attack target.
    #[test]
    fn does_not_flag_hmm_dimension_count() {
        let src = r#"
fn validate(transition: &Mat, observation: &Mat, initial: &Vec) {
    let (an0, an1) = transition.dim();
    let (bn, bm) = observation.dim();
    let pin = initial.dim();
    if an0 != an1 || an0 != bn || an0 != pin {
        return;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// An integer-annotated local bound to a count is numeric, not a credential.
    #[test]
    fn does_not_flag_integer_annotated_pin() {
        let src = "fn f(n: usize) -> bool { let pin: usize = n; pin != n }";
        assert!(run_on(src).is_empty());
    }

    /// Comparing a sensitively-named value against an integer literal is an
    /// integer comparison (constant-time), so it is not a timing-attack target.
    #[test]
    fn does_not_flag_pin_against_integer_literal() {
        let src = "fn f() -> bool { let pin = compute(); pin == 4 }";
        assert!(run_on(src).is_empty());
    }

    /// Over-exemption guard: a genuine string PIN compared byte-by-byte is not a
    /// scalar integer, so it must still flag.
    #[test]
    fn flags_string_pin_comparison() {
        let src = "fn f(stored_pin: &str, entered: &str) -> bool { stored_pin == entered }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// Over-exemption guard: `+` is `Add` (string concatenation yields a
    /// `String`), so a secret bound to `prefix + suffix` is not a count and must
    /// still flag — the integer exemption keys on length / dimension accessors,
    /// not on arithmetic operators.
    #[test]
    fn flags_secret_bound_to_string_concat() {
        let src =
            "fn f(prefix: String, suffix: &str, input: &str) -> bool { let secret = prefix + suffix; secret == input }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// jdx/mise crates/mise-sigstore/src/lib.rs:767 (#6809) — `digest.digest`
    /// is a sigstore bundle's content-addressable SHA-256, compared against the
    /// artifact's own public hash. `digest` is the OCI/sigstore content-hash
    /// term, not a credential, so neither operand is a secret.
    #[test]
    fn does_not_flag_sigstore_bundle_digest() {
        let src = "fn verify(digest: &Digest, artifact_hash: &str) -> bool { digest.digest != artifact_hash }";
        assert!(run_on(src).is_empty());
    }

    /// jdx/mise src/oci/layout.rs:70 (#6809) — an OCI blob content-integrity
    /// check; `actual` and `digest` are both public SHA-256 fingerprints.
    #[test]
    fn does_not_flag_oci_blob_digest_mismatch() {
        let src = "fn check(actual: &str, digest: &str) -> bool { actual != digest }";
        assert!(run_on(src).is_empty());
    }

    /// Over-exemption guard: a credential-qualified digest (auth context)
    /// carries a crypto qualifier and stays flagged — only the bare /
    /// content-addressed `digest` is exempt.
    #[test]
    fn flags_auth_digest_comparison() {
        let src = "fn f(auth_digest: &str, expected_digest: &str) -> bool { auth_digest != expected_digest }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// windmill-common/src/lib.rs:1356 (#7783) — `"iamrds"` is a public sentinel
    /// selecting an auth mode, not a hidden secret; the RHS is a string literal
    /// (a compile-time constant), so the comparison has no timing-attack surface.
    #[test]
    fn does_not_flag_password_against_string_literal_sentinel() {
        let src = r#"fn f(password: &str) -> bool { password == "iamrds" }"#;
        assert!(run_on(src).is_empty());
    }

    /// windmill-common/src/lib.rs:1386 (#7783) — the `!=` / `entraid` variant.
    #[test]
    fn does_not_flag_password_not_equal_string_literal() {
        let src = r#"fn f(password: &str) -> bool { password != "entraid" }"#;
        assert!(run_on(src).is_empty());
    }

    /// A sensitively-named value compared against a char literal is compared
    /// against a public compile-time constant — no secret to leak.
    #[test]
    fn does_not_flag_secret_against_char_literal() {
        let src = "fn f(secret: char) -> bool { secret == 'x' }";
        assert!(run_on(src).is_empty());
    }

    /// A raw string literal is likewise a public compile-time constant; the LHS
    /// is a sensitive identifier so the exemption (not the non-sensitive early
    /// return) is what suppresses the diagnostic.
    #[test]
    fn does_not_flag_password_against_raw_string_literal() {
        let src = r##"fn f(password: &str) -> bool { password == r"abc" }"##;
        assert!(run_on(src).is_empty());
    }

    /// The uv lockfile self-consistency check of #6855: the source mandates a
    /// hash field, and the wheel must carry one. Both operands are `bool`, and
    /// the words of `requires_hash` hold no cryptographic qualifier.
    #[test]
    fn does_not_flag_uv_requires_hash_consistency_check() {
        let src = r#"
fn validate(dist: &Dist) -> Result<(), LockError> {
    if let Some(requires_hash) = dist.id.source.requires_hash() {
        for wheel in &dist.wheels {
            if requires_hash != wheel.hash.is_some() {
                return Err(LockErrorKind::Hash.into());
            }
        }
    }
    Ok(())
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// The same name outside a boolean comparison is still not a credential:
    /// the `sha` qualifier only spans the `require|sha|sh` seam.
    #[test]
    fn does_not_flag_requires_hash_string_comparison() {
        let src = "fn f(requires_hash: &str, other: &str) -> bool { requires_hash == other }";
        assert!(run_on(src).is_empty());
    }

    /// A property of a secret is one bit, not the secret: `.is_some()` proves
    /// both operands are `bool`. The `password_hash` field is sensitive and
    /// carries no visible type, so the boolean exemption is what suppresses the
    /// diagnostic.
    #[test]
    fn does_not_flag_boolean_property_of_secret() {
        let src = "fn f(user: &User) -> bool { user.session.is_some() == user.password_hash }";
        assert!(run_on(src).is_empty());
    }

    /// A negated boolean predicate is still a `bool`.
    #[test]
    fn does_not_flag_negated_boolean_predicate() {
        let src = "fn f(user: &User, hashes: &[Hash]) -> bool { user.api_key == !hashes.is_empty() }";
        assert!(run_on(src).is_empty());
    }

    /// Over-exemption guard: `*` and `-` are also `unary_expression`, and a
    /// dereferenced credential is no `bool`, so it must still flag.
    #[test]
    fn flags_credential_against_dereferenced_operand() {
        let src =
            "fn f(password_hash: &str, expected: &&str) -> bool { password_hash == *expected }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// Over-exemption guard: `!` is bitwise NOT on anything that is not a
    /// `bool`, so a negated bit mask stays a multi-byte value and flags.
    #[test]
    fn flags_credential_against_bitwise_negation() {
        let src = "fn f(password_hash: &str, flags: Flags) -> bool { password_hash == !flags.bits() }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// A parenthesized comparison yields a `bool`.
    #[test]
    fn does_not_flag_comparison_against_nested_comparison() {
        let src = "fn f(user: &User, count: usize) -> bool { user.api_key == (count > 0) }";
        assert!(run_on(src).is_empty());
    }

    /// A boolean literal operand pins both sides to `bool`.
    #[test]
    fn does_not_flag_secret_flag_against_boolean_literal() {
        let src = "fn f(user: &User) -> bool { user.api_key == true }";
        assert!(run_on(src).is_empty());
    }

    /// Over-exemption guard: without the `.is_some()` the same operands are
    /// compared byte by byte, which must still flag.
    #[test]
    fn flags_credential_hash_comparison_without_boolean_operand() {
        let src = "fn f(user: &User, expected_hash: &str) -> bool { user.password_hash == expected_hash }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// Over-exemption guard: `secret == other_secret` has no literal operand and
    /// must still flag.
    #[test]
    fn flags_secret_against_other_secret() {
        let src = "fn f(secret: &str, other_secret: &str) -> bool { secret == other_secret }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// quinn-proto/src/tests/util.rs (#8266) — the `#[cfg(test)]` sits on the
    /// `mod tests;` declaration in the parent file, so the test file's own AST
    /// carries no gate. Test code is not attack surface.
    #[test]
    fn does_not_flag_module_gated_by_parent_cfg_test() {
        let diagnostics = crate::rules::test_helpers::run_rule_in_module_tree(
            &Check,
            &[
                ("src/lib.rs", "#[cfg(test)]\nmod tests;\n"),
                ("src/tests/mod.rs", "mod util;\n"),
                ("src/tests/util.rs", PASSWORD_COMPARISON),
            ],
        );
        assert!(diagnostics.is_empty());
    }

    /// Over-exemption guard: the same module declared without the gate ships in
    /// the release binary and must still flag.
    #[test]
    fn flags_module_declared_without_cfg_test() {
        let diagnostics = crate::rules::test_helpers::run_rule_in_module_tree(
            &Check,
            &[("src/lib.rs", "mod helpers;\n"), ("src/helpers.rs", PASSWORD_COMPARISON)],
        );
        assert_eq!(diagnostics.len(), 1);
    }

    /// quinn/tests/many_connections.rs (#8266) — a Cargo integration test is a
    /// test binary by construction; `#[cfg(test)]` is neither needed nor possible
    /// there. Its shared helper modules are test code too.
    #[test]
    fn does_not_flag_cargo_integration_test_target() {
        for target in ["tests/it.rs", "tests/common/mod.rs"] {
            let diagnostics = crate::rules::test_helpers::run_rule_in_module_tree(
                &Check,
                &[
                    ("Cargo.toml", crate::rules::test_helpers::LIB_CARGO_TOML),
                    ("src/lib.rs", ""),
                    (target, PASSWORD_COMPARISON),
                ],
            );
            assert!(diagnostics.is_empty(), "{target} is test-only code");
        }
    }

    /// Over-exemption guard: the `tests/` directory is anchored on the package
    /// root, so a package that merely lives under a directory named `tests` is
    /// ordinary shipped code.
    #[test]
    fn flags_package_nested_under_a_tests_directory() {
        let diagnostics = crate::rules::test_helpers::run_rule_in_module_tree(
            &Check,
            &[
                ("tests/pkg/Cargo.toml", crate::rules::test_helpers::LIB_CARGO_TOML),
                ("tests/pkg/src/lib.rs", PASSWORD_COMPARISON),
            ],
        );
        assert_eq!(diagnostics.len(), 1);
    }

    /// Over-exemption guard: a `src/tests/` module is an ordinary module unless
    /// its declaration is gated, so the `tests` segment alone exempts nothing.
    #[test]
    fn flags_src_tests_directory_without_gate() {
        let diagnostics = crate::rules::test_helpers::run_rule_in_module_tree(
            &Check,
            &[
                ("src/lib.rs", "mod tests;\n"),
                ("src/tests/mod.rs", "mod util;\n"),
                ("src/tests/util.rs", PASSWORD_COMPARISON),
            ],
        );
        assert_eq!(diagnostics.len(), 1);
    }
}

//! no-weak-cipher Rust backend.
//!
//! Rust crypto libraries select the cipher by **method name**, not by a
//! string argument. The `openssl` crate exposes weak primitives via
//! `openssl::symm::Cipher::des_ecb()`, `Cipher::rc4()`, `Cipher::bf_cbc()`,
//! etc. The rule matches `call_expression` nodes whose function is a
//! `scoped_identifier` of the form `[<path>::]Cipher::<weak_name>`.
//!
//! `<weak_name>` is any identifier whose name starts with a weak-cipher
//! family prefix (`des`, `rc4`, `rc2`, `bf`, `blowfish`) followed by
//! either end-of-identifier or `_`. That covers `des_ecb`, `des_cbc`,
//! `des_ede3_cbc`, `rc4`, `rc4_40`, `bf_cbc`, etc., without matching
//! `describe`, `rc42`, `bfs`, or similar.
//!
//! Gating on `Cipher` as the immediate preceding path segment avoids
//! flagging unrelated code that happens to define a function named
//! `des_ecb` in an unrelated module.

use crate::diagnostic::{Diagnostic, Severity};

const WEAK_PREFIXES: &[&str] = &["des", "rc4", "rc2", "bf", "blowfish"];

fn is_weak_cipher_method(name: &str) -> bool {
    WEAK_PREFIXES.iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('_'))
    })
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(function) = node.child_by_field_name("function") else { return };
    if function.kind() != "scoped_identifier" {
        return;
    }
    let Some(name) = function.child_by_field_name("name") else { return };
    let Ok(method_name) = name.utf8_text(source) else { return };
    if !is_weak_cipher_method(method_name) {
        return;
    }
    let Some(path) = function.child_by_field_name("path") else { return };
    let last_path_segment = match path.kind() {
        "identifier" => path.utf8_text(source).ok(),
        "scoped_identifier" => path
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok()),
        _ => None,
    };
    if last_path_segment != Some("Cipher") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-weak-cipher".into(),
        message: format!(
            "Weak cipher `Cipher::{method_name}` \u{2014} use `Cipher::aes_256_gcm()` or ChaCha20-Poly1305."
        ),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_cipher_des_ecb() {
        assert_eq!(run_on(r#"fn f() { let c = Cipher::des_ecb(); }"#).len(), 1);
    }

    #[test]
    fn flags_cipher_des_ede3_cbc() {
        assert_eq!(
            run_on(r#"fn f() { let c = Cipher::des_ede3_cbc(); }"#).len(),
            1,
        );
    }

    #[test]
    fn flags_fully_qualified_cipher() {
        assert_eq!(
            run_on(r#"fn f() { let c = openssl::symm::Cipher::des_cbc(); }"#).len(),
            1,
        );
    }

    #[test]
    fn flags_cipher_rc4() {
        assert_eq!(run_on(r#"fn f() { let c = Cipher::rc4(); }"#).len(), 1);
    }

    #[test]
    fn flags_cipher_bf_cbc() {
        assert_eq!(run_on(r#"fn f() { let c = Cipher::bf_cbc(); }"#).len(), 1);
    }

    #[test]
    fn flags_cipher_rc2_cbc() {
        assert_eq!(run_on(r#"fn f() { let c = Cipher::rc2_cbc(); }"#).len(), 1);
    }

    #[test]
    fn allows_cipher_aes_256_gcm() {
        assert!(run_on(r#"fn f() { let c = Cipher::aes_256_gcm(); }"#).is_empty());
    }

    #[test]
    fn allows_unrelated_method_starting_with_des_prefix() {
        // `Cipher::describe()` starts with "des" but the rest doesn't
        // begin with `_` or end there — not a cipher family name.
        assert!(run_on(r#"fn f() { Cipher::describe(); }"#).is_empty());
    }

    #[test]
    fn allows_des_ecb_outside_cipher_scope() {
        // `my_utils::des_ecb()` doesn't go through the `Cipher` type.
        // Unrelated function that happens to share the name.
        assert!(run_on(r#"fn f() { my_utils::des_ecb(); }"#).is_empty());
    }

    #[test]
    fn allows_plain_function_des_ecb() {
        // Plain `des_ecb()` (unqualified) — not a scoped_identifier call,
        // just a local function with a name that starts with a cipher
        // prefix. Not flagged.
        assert!(run_on(r#"fn f() { des_ecb(); }"#).is_empty());
    }

    /// The original FP: a string literal like
    /// `"jsdoc-require-throws-description"` must not trip the rule,
    /// because the new backend does not inspect string contents at all.
    #[test]
    fn does_not_flag_random_string_literals() {
        let src = r#"
pub struct Meta<'a> { pub id: &'a str, pub doc_url: Option<&'a str> }
pub const META: Meta<'static> = Meta {
    id: "jsdoc-require-throws-description",
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-throws-description.md"),
};
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn unit_is_weak_cipher_method() {
        assert!(is_weak_cipher_method("des"));
        assert!(is_weak_cipher_method("des_ecb"));
        assert!(is_weak_cipher_method("des_ede3_cbc"));
        assert!(is_weak_cipher_method("rc4"));
        assert!(is_weak_cipher_method("rc4_40"));
        assert!(is_weak_cipher_method("rc2_cbc"));
        assert!(is_weak_cipher_method("bf"));
        assert!(is_weak_cipher_method("bf_cbc"));
        assert!(is_weak_cipher_method("blowfish"));
        assert!(!is_weak_cipher_method("describe"));
        assert!(!is_weak_cipher_method("description"));
        assert!(!is_weak_cipher_method("designed"));
        assert!(!is_weak_cipher_method("rc42"));
        assert!(!is_weak_cipher_method("bfs"));
        assert!(!is_weak_cipher_method("aes_256_gcm"));
    }
}

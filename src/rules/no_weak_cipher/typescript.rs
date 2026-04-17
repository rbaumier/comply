//! no-weak-cipher TypeScript / JavaScript / TSX backend.
//!
//! Node.js crypto selects the cipher via a string argument to
//! `crypto.createCipheriv(algo, key, iv)`. The rule walks
//! `call_expression` nodes whose callee's trailing name is
//! `createCipheriv` and whose first argument is a string literal
//! starting with a weak-cipher family prefix (`bf`, `blowfish`, `des`,
//! `rc2`, `rc4`), matching SonarJS rule S5547.
//!
//! No fully-qualified-name / constant-propagation resolution: a call
//! whose first argument is an `identifier` (e.g.
//! `createCipheriv(algo, key, iv)` where `algo` is a `const` declared
//! elsewhere) is not flagged. This is a known gap that could be closed
//! with a file-level scan for the binding, but the common-case use is
//! a literal in the call site.

use crate::diagnostic::{Diagnostic, Severity};

const WEAK_PREFIXES: &[&str] = &["bf", "blowfish", "des", "rc2", "rc4"];

fn is_weak_cipher_spec(value: &str) -> bool {
    WEAK_PREFIXES
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(function) = node.child_by_field_name("function") else { return };
    let callee_name = match function.kind() {
        "identifier" => function.utf8_text(source).ok(),
        "member_expression" => function
            .child_by_field_name("property")
            .and_then(|p| p.utf8_text(source).ok()),
        _ => None,
    };
    if callee_name != Some("createCipheriv") {
        return;
    }
    let Some(arguments) = node.child_by_field_name("arguments") else { return };
    let mut cursor = arguments.walk();
    let Some(first_arg) = arguments.named_children(&mut cursor).next() else { return };
    if first_arg.kind() != "string" {
        return;
    }
    // Concatenate the string_fragment children (a template-less `string`
    // node in tree-sitter-typescript has one or two fragments for
    // escape splits). Reject if the arg has any non-fragment children
    // (shouldn't happen for a plain string, but keeps things defensive).
    let mut fragments = String::new();
    let mut fragment_cursor = first_arg.walk();
    for child in first_arg.named_children(&mut fragment_cursor) {
        if child.kind() == "string_fragment"
            && let Ok(t) = child.utf8_text(source) {
                fragments.push_str(t);
            }
    }
    let lowered = fragments.to_ascii_lowercase();
    if !is_weak_cipher_spec(&lowered) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-weak-cipher",
        format!(
            "Weak cipher `{fragments}` passed to `createCipheriv` \u{2014} use `aes-256-gcm` or ChaCha20-Poly1305."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_createcipheriv_des_ecb() {
        let src = r#"const c = crypto.createCipheriv("des-ecb", key, iv);"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_createcipheriv_rc4() {
        let src = r#"const c = crypto.createCipheriv("rc4", key, iv);"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_createcipheriv_blowfish() {
        let src = r#"const c = crypto.createCipheriv("blowfish", key, iv);"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bare_createcipheriv_call() {
        // Imported via `import { createCipheriv } from 'crypto';`
        let src = r#"const c = createCipheriv("des-cbc", key, iv);"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_createcipheriv_aes_256_gcm() {
        let src = r#"const c = crypto.createCipheriv("aes-256-gcm", key, iv);"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unrelated_string_outside_createcipheriv() {
        let src = r#"const id = "jsdoc-require-throws-description";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_createcipheriv_with_non_literal_arg() {
        // Variable reference — we don't do constant propagation v1.
        let src = r#"const c = crypto.createCipheriv(algo, key, iv);"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unrelated_call_with_des_string() {
        // `console.log("des-ecb")` is obviously not a crypto call.
        let src = r#"console.log("des-ecb");"#;
        assert!(run_on(src).is_empty());
    }
}

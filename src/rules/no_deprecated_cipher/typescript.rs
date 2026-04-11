//! no-deprecated-cipher — flag `createCipher()` calls (but not
//! `createCipheriv()`).
//!
//! Matches `call_expression` nodes where the callee ends with
//! `createCipher` (either as a bare call or `crypto.createCipher`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };

    let method_name = match callee.kind() {
        "member_expression" => {
            let Some(prop) = callee.child_by_field_name("property") else { return };
            prop.utf8_text(source).ok()
        }
        "identifier" => callee.utf8_text(source).ok(),
        _ => None,
    };

    let Some(name) = method_name else { return };

    // Match exactly "createCipher" but NOT "createCipheriv" etc.
    if name != "createCipher" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-deprecated-cipher".into(),
        message: "`createCipher()` is deprecated — use `createCipheriv()` with an explicit IV.".into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_create_cipher() {
        assert_eq!(
            run_on("const cipher = crypto.createCipher('aes256', password);").len(),
            1
        );
    }

    #[test]
    fn flags_bare_create_cipher() {
        assert_eq!(run_on("createCipher('aes-128-cbc', pw)").len(), 1);
    }

    #[test]
    fn allows_create_cipheriv() {
        assert!(run_on("const cipher = crypto.createCipheriv('aes-256-cbc', key, iv);").is_empty());
    }

    #[test]
    fn allows_unrelated_code() {
        assert!(run_on("const x = encrypt(data);").is_empty());
    }
}

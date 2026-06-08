//! no-weak-cipher OXC backend — flag weak ciphers in `createCipheriv` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

const WEAK_PREFIXES: &[&str] = &["bf", "blowfish", "des", "rc2", "rc4"];

fn is_weak_cipher_spec(value: &str) -> bool {
    WEAK_PREFIXES.iter().any(|prefix| value.starts_with(prefix))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["des", "rc2", "rc4", "blowfish", "DES", "RC2", "RC4"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(member) => member.property.name.as_str(),
            _ => return,
        };
        if callee_name != "createCipheriv" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Argument::StringLiteral(lit) = first_arg else {
            return;
        };

        let lowered = lit.value.as_str().to_ascii_lowercase();
        if !is_weak_cipher_spec(&lowered) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Weak cipher `{}` passed to `createCipheriv` \u{2014} use `aes-256-gcm` or ChaCha20-Poly1305.",
                lit.value
            ),
            severity: Severity::Error,
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

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

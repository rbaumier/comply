//! security-detect-insecure-randomness oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Security-context identifier names — when the result of
/// `Math.random()` flows into one of these, it's almost certainly a
/// real cryptographic-security mistake.
const SECURITY_CONTEXT_NAMES: &[&str] = &[
    "token",
    "tokens",
    "secret",
    "password",
    "passwd",
    "passphrase",
    "session",
    "sessionId",
    "sessionToken",
    "apiKey",
    "key",
    "salt",
    "nonce",
    "id",
    "uuid",
    "csrf",
    "csrfToken",
    "resetCode",
    "verification",
    "verificationCode",
    "otp",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Math.random"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Math" || member.property.name.as_str() != "random" {
            return;
        }
        // Walk up to find the enclosing VariableDeclarator / FormalParameter /
        // ReturnStatement and check if the surrounding identifier matches a
        // security context.
        let mut current_id = node.id();
        let mut hit_security_context = false;
        for _ in 0..6 {
            let parent_id = semantic.nodes().parent_id(current_id);
            if parent_id == current_id {
                break;
            }
            let parent = semantic.nodes().get_node(parent_id);
            match parent.kind() {
                AstKind::VariableDeclarator(decl) => {
                    if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id
                        && SECURITY_CONTEXT_NAMES.contains(&id.name.as_str())
                    {
                        hit_security_context = true;
                    }
                    break;
                }
                AstKind::ObjectProperty(p) => {
                    if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key
                        && SECURITY_CONTEXT_NAMES.contains(&key.name.as_str())
                    {
                        hit_security_context = true;
                    }
                    break;
                }
                _ => {}
            }
            current_id = parent_id;
        }
        if !hit_security_context {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Math.random()` is not cryptographically secure — flowing it \
                      into a token / secret / session id is exploitable. Use \
                      `crypto.randomUUID()` / `crypto.getRandomValues(...)` instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_math_random_into_token() {
        let src = r#"const token = Math.random().toString(36);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_math_random_into_sessionid() {
        let src = r#"const sessionId = Math.random();"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_math_random_for_non_security_context() {
        let src = r#"const delay = Math.random() * 1000;"#;
        assert!(run(src).is_empty());
    }
}

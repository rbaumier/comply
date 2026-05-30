//! zod-trim-before-min OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use std::sync::Arc;

/// Field names where leading/trailing whitespace is meaningful and trimming
/// would silently corrupt the value. The schema is typically a probe over the
/// raw payload (passwords forwarded to an auth lib, tokens compared verbatim,
/// secrets used as opaque bytes). Adding `.trim()` would diverge the validated
/// value from the stored/compared value and break authentication.
const WHITESPACE_SENSITIVE_SUBSTRINGS: &[&str] = &[
    "password",
    "token",
    "secret",
    "apikey",
    "api_key",
    "jwt",
    "signature",
    "otp",
    "passcode",
    "pincode",
    "twofactor",
    "two_factor",
    "2fa",
    "mfa",
    "verificationcode",
    "verification_code",
];

fn is_whitespace_sensitive_key(name: &str) -> bool {
    let lower = name.to_lowercase();
    WHITESPACE_SENSITIVE_SUBSTRINGS.iter().any(|needle| lower.contains(needle))
}

fn prop_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Walk back through a method-chain (CallExpression whose callee is a
/// StaticMemberExpression whose object is another CallExpression ...) and
/// collect every method name. Returns `None` if the chain does not bottom
/// out at a `z.string()` call.
fn collect_chain<'a>(expr: &'a Expression<'a>, ctx: &CheckCtx) -> Option<Vec<&'a str>> {
    let mut methods = Vec::new();
    let mut cur = expr;
    loop {
        let Expression::CallExpression(call) = cur else { return None };
        match &call.callee {
            Expression::StaticMemberExpression(member) => {
                let callee_text = &ctx.source[member.span.start as usize..member.span.end as usize];
                if callee_text == "z.string" {
                    return Some(methods);
                }
                methods.push(member.property.name.as_str());
                cur = &member.object;
            }
            _ => return None,
        }
    }
}

/// True when the nearest naming context (object-property key or variable
/// binding name) implies that whitespace is meaningful. Stops at the first
/// relevant ancestor so property keys take precedence over outer variable names.
fn schema_context_is_whitespace_sensitive(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node_id) {
        match ancestor.kind() {
            AstKind::ObjectProperty(prop) => {
                if let Some(name) = prop_key_name(&prop.key) {
                    return is_whitespace_sensitive_key(name);
                }
                return false;
            }
            AstKind::VariableDeclarator(decl) => {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id {
                    return is_whitespace_sensitive_key(id.name.as_str());
                }
                return false;
            }
            _ => {}
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Only fire on the `.min(...)` call itself.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "min" { return; }

        // The receiver chain must reach `z.string()`.
        let Some(methods) = collect_chain(&member.object, ctx) else { return };

        // If `.trim()` appears anywhere in the chain (before `.min`), no warning.
        if methods.contains(&"trim") { return; }

        // Skip when the naming context (property key or variable name) implies
        // whitespace is meaningful (passwords, tokens, secrets, ...). Trimming
        // would diverge the validated value from what is stored/compared downstream.
        if schema_context_is_whitespace_sensitive(node.id(), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Add `.trim()` before `.min()` — `z.string().min(1)` allows whitespace-only strings.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_min_without_trim() {
        assert_eq!(run("z.string().min(1)").len(), 1);
    }

    #[test]
    fn allows_trim_before_min() {
        assert!(run("z.string().trim().min(1)").is_empty());
    }

    #[test]
    fn skips_new_password_field_from_issue_176() {
        let src = r#"
            const NewPasswordBodySchema = z.looseObject({
                newPassword: z.string().min(1).max(128),
            });
        "#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_password_field() {
        let src = "const s = z.object({ password: z.string().min(8) });";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_whitespace_sensitive_variants() {
        for field in &[
            "currentPassword",
            "confirmPassword",
            "passwordHash",
            "apiKey",
            "accessToken",
            "refreshToken",
            "idToken",
            "secret",
            "clientSecret",
            "jwt",
            "signature",
            "otp",
            "twoFactorCode",
        ] {
            let src = format!("const s = z.object({{ {field}: z.string().min(1) }});");
            let diags = run(&src);
            assert!(
                diags.is_empty(),
                "expected no diagnostic for field `{field}`, got: {diags:?}",
            );
        }
    }

    #[test]
    fn still_flags_regular_text_field_named_name() {
        let src = "const s = z.object({ name: z.string().min(1) });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_email_field() {
        let src = "const s = z.object({ email: z.string().min(1).max(255) });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_standalone_password_schema_from_issue_507() {
        let src = "export const PasswordFieldSchema = z.string().min(10).max(128);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_standalone_token_schema_by_variable_name() {
        let src = "const accessTokenSchema = z.string().min(20);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn still_flags_standalone_schema_with_regular_name() {
        let src = "const UsernameSchema = z.string().min(3);";
        assert_eq!(run(src).len(), 1);
    }
}

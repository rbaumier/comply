//! zod-trim-before-min backend — flag `z.string().min(...)` chains that
//! omit `.trim()`. Walks the chain of method calls anchored on a
//! `z.string()` call to determine which methods appear before/around
//! the `.min(...)` call.

use crate::diagnostic::{Diagnostic, Severity};

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
    WHITESPACE_SENSITIVE_SUBSTRINGS
        .iter()
        .any(|needle| lower.contains(needle))
}

/// Strip matching surrounding quote characters from a property key.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// True when the nearest naming context (object `pair` key or `variable_declarator`
/// binding name) implies that whitespace is meaningful. Stops at the first relevant
/// ancestor so property keys take precedence over outer variable names.
fn schema_context_is_whitespace_sensitive(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "pair" => {
                let Some(key_node) = p.child_by_field_name("key") else {
                    return false;
                };
                let key_text = unquote(key_node.utf8_text(source).unwrap_or(""));
                return is_whitespace_sensitive_key(key_text);
            }
            "variable_declarator" => {
                let Some(name_node) = p.child_by_field_name("name") else {
                    return false;
                };
                let name_text = name_node.utf8_text(source).unwrap_or("");
                return is_whitespace_sensitive_key(name_text);
            }
            _ => {}
        }
        cur = p.parent();
    }
    false
}

/// Walk back through a method-chain (call_expression → member_expression
/// whose object is itself a call_expression …) and collect every method
/// name encountered. Returns `None` if the chain does not bottom out at a
/// `z.string()` call.
fn collect_chain<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<Vec<&'a str>> {
    let mut methods = Vec::new();
    let mut cur = node;
    loop {
        if cur.kind() != "call_expression" {
            return None;
        }
        let function = cur.child_by_field_name("function")?;
        // `z.string()` itself: function is the member_expression `z.string`
        // (object=`z` identifier, property=`string`).
        if function.kind() == "member_expression" {
            let function_text = function.utf8_text(source).ok()?;
            if function_text == "z.string" {
                return Some(methods);
            }
            // Otherwise: chained method call. Record property name, descend
            // into the receiver (member_expression `object`).
            let property = function.child_by_field_name("property")?;
            let name = property.utf8_text(source).ok()?;
            methods.push(name);
            cur = function.child_by_field_name("object")?;
            continue;
        }
        return None;
    }
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Only fire on the `.min(...)` call itself.
    let Some(function) = node.child_by_field_name("function") else { return };
    if function.kind() != "member_expression" { return; }
    let Some(property) = function.child_by_field_name("property") else { return };
    let Ok(method) = property.utf8_text(source) else { return };
    if method != "min" { return; }

    // The receiver chain must reach `z.string()`.
    let Some(object) = function.child_by_field_name("object") else { return };
    let Some(methods) = collect_chain(object, source) else { return };

    // If `.trim()` appears anywhere in the chain (before `.min`), no warning.
    if methods.iter().any(|m| *m == "trim") { return; }

    // Skip when the naming context (property key or variable name) implies
    // whitespace is meaningful (passwords, tokens, secrets, ...). Trimming
    // would diverge the validated value from what is stored/compared downstream.
    if schema_context_is_whitespace_sensitive(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Add `.trim()` before `.min()` — `z.string().min(1)` allows whitespace-only strings.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
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
        // Repro from the issue: trimming would diverge the validated value
        // from the value Better Auth stores verbatim.
        let src = r#"
            const NewPasswordBodySchema = z.looseObject({
                newPassword: z.string().min(1).max(128),
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_password_field() {
        let src = "const s = z.object({ password: z.string().min(8) });";
        assert!(run(src).is_empty());
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
            assert!(
                run(&src).is_empty(),
                "expected no diagnostic for field `{field}`, got: {:?}",
                run(&src),
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

//! OXC backend for better-auth-required-user-fields — require `email` and
//! `name` in the `user` config object.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e.", ".integration."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Recursively check if an object expression contains a property with the given key name.
fn has_property_key(expr: &Expression<'_>, name: &str) -> bool {
    let Expression::ObjectExpression(obj) = expr else { return false };
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        if property_key_matches(&p.key, name) {
            return true;
        }
        // Recurse into nested objects.
        if has_property_key(&p.value, name) {
            return true;
        }
    }
    false
}

fn property_key_matches(key: &PropertyKey<'_>, name: &str) -> bool {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name == name,
        PropertyKey::StringLiteral(lit) => lit.value == name,
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if is_test_file(ctx.path) {
            return Vec::new();
        }
        use oxc_ast::AstKind;
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ObjectProperty(prop) = node.kind() else { continue };
            if !property_key_matches(&prop.key, "user") {
                continue;
            }
            let Expression::ObjectExpression(_) = &prop.value else { continue };

            let has_email = has_property_key(&prop.value, "email");
            let has_name = has_property_key(&prop.value, "name");

            if has_email && has_name {
                continue;
            }

            let missing = match (has_email, has_name) {
                (false, false) => "`email` and `name`",
                (false, true) => "`email`",
                (true, false) => "`name`",
                _ => continue,
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, prop.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`user` schema is missing {missing} — both fields are required."),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    fn run_with_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_missing_both_in_prod_file() {
        let src = "betterAuth({ user: { additionalFields: { role: { type: 'string' } } } })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_email_and_name() {
        let src = "betterAuth({ user: { additionalFields: { email: {}, name: {} } } })";
        assert!(run(src).is_empty());
    }

    // Regression for #448: partial fixtures in test files must not trigger the rule.
    #[test]
    fn no_fp_on_partial_object_in_test_file() {
        let src = "const testUser = { id: crypto.randomUUID(), email: 'test@example.com' };";
        assert!(run_with_path(src, "auth.test.ts").is_empty());
    }

    #[test]
    fn no_fp_on_partial_object_in_spec_file() {
        let src = "const testUser = { id: crypto.randomUUID(), email: 'test@example.com' };";
        assert!(run_with_path(src, "auth.spec.ts").is_empty());
    }

    #[test]
    fn no_fp_on_partial_object_in_integration_test_file() {
        let src = "const testUser = { id: crypto.randomUUID(), email: 'test@example.com' };";
        assert!(run_with_path(src, "user.integration.test.ts").is_empty());
    }

    #[test]
    fn still_flags_prod_file_with_user_config_missing_fields() {
        let src = "betterAuth({ user: { additionalFields: { role: { type: 'string' } } } })";
        assert_eq!(run_with_path(src, "auth.config.ts").len(), 1);
    }
}

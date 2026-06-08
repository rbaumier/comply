use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn prop_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

fn object_has_key(obj: &oxc_ast::ast::ObjectExpression, needle: &str) -> bool {
    obj.properties.iter().any(|p| {
        let ObjectPropertyKind::ObjectProperty(prop) = p else {
            return false;
        };
        prop_key_name(&prop.key) == Some(needle)
    })
}

fn prop_value_is_true(prop: &oxc_ast::ast::ObjectProperty) -> bool {
    matches!(&prop.value, Expression::BooleanLiteral(b) if b.value)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };
        if prop_key_name(&prop.key) != Some("emailAndPassword") {
            return;
        }
        let Expression::ObjectExpression(obj) = &prop.value else {
            return;
        };

        // Check enabled: true
        let has_enabled_true = obj.properties.iter().any(|p| {
            let ObjectPropertyKind::ObjectProperty(inner) = p else {
                return false;
            };
            prop_key_name(&inner.key) == Some("enabled") && prop_value_is_true(inner)
        });
        if !has_enabled_true {
            return;
        }

        if object_has_key(obj, "sendResetPassword") {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`emailAndPassword.enabled: true` requires a `sendResetPassword` handler."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_missing_handler() {
        let src = "betterAuth({ emailAndPassword: { enabled: true } })";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_with_handler() {
        let src = "betterAuth({ emailAndPassword: { enabled: true, sendResetPassword: async (x) => {} } })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_when_disabled() {
        let src = "betterAuth({ emailAndPassword: { enabled: false } })";
        assert!(run(src).is_empty());
    }
}

//! OXC backend for better-auth-required-user-fields — require `email` and
//! `name` in the `user` config object.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

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

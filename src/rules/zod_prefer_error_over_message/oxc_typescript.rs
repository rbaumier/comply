//! zod-prefer-error-over-message oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, ObjectExpression, ObjectProperty, ObjectPropertyKind, PropertyKey,
};
use std::sync::Arc;

pub struct Check;

/// True when the root of a member/call chain is the bare identifier `z`,
/// i.e. `z.string(...)`, `z.string().min(...)`, `z.object({...})`.
fn chain_root_is_z(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == "z",
        Expression::StaticMemberExpression(m) => chain_root_is_z(&m.object),
        Expression::ComputedMemberExpression(m) => chain_root_is_z(&m.object),
        Expression::CallExpression(c) => chain_root_is_z(&c.callee),
        _ => false,
    }
}

fn prop_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// A `message:` property whose value is a plain string/template literal — that's
/// a Zod error-customization string, not a `message: z.string()` schema field.
fn find_string_message_prop<'a, 'b>(
    obj: &'b ObjectExpression<'a>,
) -> Option<&'b ObjectProperty<'a>> {
    for p in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = p else { continue };
        if prop_key_name(&prop.key) == Some("message")
            && matches!(
                prop.value,
                Expression::StringLiteral(_) | Expression::TemplateLiteral(_)
            )
        {
            return Some(prop);
        }
    }
    None
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["message"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Only `z.<method>(...)` member calls — never a bare `z(...)`, which
        // would catch any local variable that happens to be named `z`.
        if !matches!(
            call.callee,
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
        ) {
            return;
        }
        if !chain_root_is_z(&call.callee) {
            return;
        }

        for arg in &call.arguments {
            let Some(Expression::ObjectExpression(obj)) = arg.as_expression() else {
                continue;
            };
            let Some(prop) = find_string_message_prop(obj) else { continue };

            let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Zod v4 renamed `message` to `error`. Use `error: '...'` instead of \
                          `message: '...'` in this `z.*` call."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_string_message_in_z_string() {
        let src = r#"const s = z.string({ message: "Requis" });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_message_in_chained_validator() {
        let src = r#"const s = z.string().min(1, { message: "Trop court" });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_template_literal_message() {
        let src = r#"const s = z.string().max(n, { message: `Max ${n}` });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_error_key() {
        let src = r#"const s = z.string({ error: "Requis" });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_message_schema_field() {
        // `message` here is a schema field, not an error-customization string.
        let src = r#"const s = z.object({ message: z.string() });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_zod_chain() {
        // Root is not `z`, so this is some other builder.
        let src = r#"const s = toast.error({ message: "Oops" });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn regression_amadeo_zod_v4_message() {
        // amadeo migrated to Zod v4: error-customization strings use `error`,
        // not the deprecated v3 `message` key.
        let src = r#"
            const schema = z.object({
              name: z.string().min(1, { message: "Le nom est requis" }),
              email: z.string().email({ message: "Email invalide" }),
            });
        "#;
        assert_eq!(run(src).len(), 2);
    }
}

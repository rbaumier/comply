//! zod-prefer-error-over-message oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    CallExpression, Expression, ObjectExpression, ObjectProperty, ObjectPropertyKind, PropertyKey,
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

/// The named property whose value is a plain string/template literal. Used to
/// find a Zod error-customization string (`message` in a `z.*` call, `error` in
/// `ctx.addIssue`) while skipping schema fields like `message: z.string()`.
fn find_string_prop<'a, 'b>(
    obj: &'b ObjectExpression<'a>,
    key: &str,
) -> Option<&'b ObjectProperty<'a>> {
    for p in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = p else { continue };
        if prop_key_name(&prop.key) == Some(key)
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

/// True for an `<obj>.addIssue(...)` call — the literal method name on a static
/// member callee. Inside `superRefine`/`.check` the receiver is a `RefinementCtx`
/// param, so we deliberately do not anchor on a `z` root.
fn is_add_issue_call(call: &CallExpression) -> bool {
    matches!(
        &call.callee,
        Expression::StaticMemberExpression(m) if m.property.name.as_str() == "addIssue"
    )
}

/// Flag a string/template `error:` key in `ctx.addIssue({ ... })`: v4 reads
/// `message` there, so an `error:` value is silently dropped.
fn flag_add_issue(call: &CallExpression, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    for arg in &call.arguments {
        let Some(Expression::ObjectExpression(obj)) = arg.as_expression() else {
            continue;
        };
        let Some(prop) = find_string_prop(obj, "error") else { continue };

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Zod v4 reads `message` (not `error`) inside `ctx.addIssue(...)`; \
                      an `error:` key is silently dropped. Use `message: '...'`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // `message` for the `z.*`/`.refine` direction; `addIssue` so a file using
        // only `ctx.addIssue({ error: ... })` (no `message` token) is not pruned.
        Some(&["message", "addIssue"])
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

        // `ctx.addIssue({ error: "..." })` — v4 wants `message` here, and an
        // `error:` key is silently dropped. Mirror image of the `z.*` check.
        if is_add_issue_call(call) {
            flag_add_issue(call, ctx, diagnostics);
            return;
        }

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
            let Some(prop) = find_string_prop(obj, "message") else { continue };

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
    fn flags_message_in_refine() {
        // `.refine(fn, { ... })` is z-rooted, so it takes `error` like other
        // `z.*` calls; a `message` key there is the deprecated v3 spelling.
        let src = r#"const s = z.string().refine((v) => v.length > 8, { message: "Too short" });"#;
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

    #[test]
    fn flags_string_error_in_add_issue() {
        // v4 reads `message` inside addIssue; an `error:` string is dropped.
        let src = r#"ctx.addIssue({ code: "custom", error: "Too short" });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_template_literal_error_in_add_issue() {
        let src = r#"ctx.addIssue({ code: "custom", error: `Too short ${n}` });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_message_in_add_issue() {
        let src = r#"ctx.addIssue({ code: "custom", message: "Too short" });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_literal_error_in_add_issue() {
        // `error: someSchema` is an identifier, not a customization string.
        let src = r#"ctx.addIssue({ error: someSchema });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_add_issue_method_with_error_string() {
        // Not `addIssue` and not a `z.*` chain — neither branch applies.
        let src = r#"logger.report({ error: "boom" });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn add_issue_only_source_fires() {
        // The issue's superRefine example: no `message` token anywhere, so the
        // file must fire on the `addIssue` branch alone.
        let src = r#"
            const s = z.string().superRefine((val, ctx) => {
              if (val.length < 3) {
                ctx.addIssue({ code: "custom", error: "Too short" });
              }
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn prefilter_includes_add_issue_token() {
        // The `addIssue`-only source above contains no `message` token, so
        // `addIssue` must be a prefilter literal or the engine would prune the
        // file before the check runs. The unit `run` helper bypasses the
        // prefilter, so assert the contract directly.
        let literals = OxcCheck::prefilter(&Check).expect("rule declares a prefilter");
        assert!(literals.contains(&"addIssue"));
    }

    #[test]
    fn computed_add_issue_does_not_fire() {
        // Only the literal method name `addIssue` matches; `ctx["addIssue"]`
        // is a computed member and is intentionally not flagged.
        let src = r#"ctx["addIssue"]({ error: "Too short" });"#;
        assert!(run(src).is_empty());
    }
}

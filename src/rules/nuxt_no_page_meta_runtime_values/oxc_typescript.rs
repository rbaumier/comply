//! nuxt-no-page-meta-runtime-values oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True for AST nodes that are safe at build time (literals + nested
/// literal containers + simple unary on literals like `-1`).
fn is_compile_time_safe(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::RegExpLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
        Expression::Identifier(id) => id.name.as_str() == "undefined",
        Expression::UnaryExpression(unary) => is_compile_time_safe(&unary.argument),
        Expression::ArrayExpression(arr) => arr.elements.iter().all(|el| match el {
            oxc_ast::ast::ArrayExpressionElement::Elision(_) => true,
            other => other
                .as_expression()
                .is_some_and(is_compile_time_safe),
        }),
        Expression::ObjectExpression(obj) => obj.properties.iter().all(|prop| match prop {
            ObjectPropertyKind::ObjectProperty(p) => is_compile_time_safe(&p.value),
            ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["definePageMeta"])
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
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "definePageMeta" {
            return;
        }
        let Some(arg) = call.arguments.first() else {
            return;
        };
        let Some(Expression::ObjectExpression(obj)) = arg.as_expression() else {
            return;
        };
        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            if is_compile_time_safe(&p.value) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, p.value.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Property value in `definePageMeta` is computed at runtime — \
                          Nuxt extracts page metadata at build time, so only literals \
                          and nested literal containers survive."
                    .into(),
                severity: Severity::Error,
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
    fn flags_variable_reference_in_meta() {
        let src = r#"
            const layout = "default";
            definePageMeta({ layout });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_function_call_in_meta() {
        let src = r#"definePageMeta({ title: getTitle() });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_literal_meta() {
        let src = r#"definePageMeta({ layout: "default", title: "Home", middleware: ["auth"] });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_nested_literal_object() {
        let src = r#"definePageMeta({ keepalive: { include: ["a", "b"] } });"#;
        assert!(run(src).is_empty());
    }
}

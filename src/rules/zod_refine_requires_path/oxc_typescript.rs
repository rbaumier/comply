//! zod-refine-requires-path OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Return true if the object expression carries a `message` key but no `path` key.
fn has_message_no_path(expr: &oxc_ast::ast::ObjectExpression) -> bool {
    let mut has_message = false;
    let mut has_path = false;
    for prop in &expr.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        let key_name = match &p.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        match key_name {
            "message" => has_message = true,
            "path" => has_path = true,
            _ => {}
        }
    }
    has_message && !has_path
}

/// Return true if the receiver source text contains `z.object(`.
fn receiver_uses_z_object(expr: &Expression, source: &str) -> bool {
    let span = expr.span();
    let text = &source[span.start as usize..span.end as usize];
    text.contains("z.object(")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["refine"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `<something>.refine`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "refine" {
            return;
        }

        // Only when receiver chain involves z.object(...)
        if !receiver_uses_z_object(&member.object, ctx.source) {
            return;
        }

        // Second argument must be an object with message but no path
        let Some(second) = call.arguments.get(1) else { return };
        let Some(Expression::ObjectExpression(obj)) = second.as_expression() else { return };
        if !has_message_no_path(obj) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Add `path: ['fieldName']` to `.refine()` options so form errors attach to the correct field.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_refine_no_path() {
        assert_eq!(
            run("z.object({ a: z.string(), b: z.string() }).refine(d => d.a !== d.b, { message: 'Must differ' })").len(),
            1
        );
    }


    #[test]
    fn allows_refine_with_path() {
        assert!(run(
            "z.object({ a: z.string() }).refine(d => d.a.length > 0, { message: 'Required', path: ['a'] })"
        )
        .is_empty());
    }
}

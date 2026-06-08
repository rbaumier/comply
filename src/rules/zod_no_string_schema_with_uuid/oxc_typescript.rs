//! zod-no-string-schema-with-uuid oxc backend — flag `z.string().uuid()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.string"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression with property `uuid`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "uuid" {
            return;
        }

        // Object must be a call expression (the `z.string()` part).
        let Expression::CallExpression(inner_call) = &member.object else {
            return;
        };

        // Inner callee must be `z.string`.
        let Expression::StaticMemberExpression(inner_member) = &inner_call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &inner_member.object else {
            return;
        };
        if obj.name.as_str() != "z" || inner_member.property.name.as_str() != "string" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `z.uuid()` instead of `z.string().uuid()` — the \
                      chained form is deprecated in Zod v4."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_z_string_uuid() {
        assert_eq!(run_on("const s = z.string().uuid();").len(), 1);
    }


    #[test]
    fn allows_z_uuid() {
        assert!(run_on("const s = z.uuid();").is_empty());
    }


    #[test]
    fn allows_z_string_email() {
        assert!(run_on("const s = z.string().email();").is_empty());
    }


    #[test]
    fn flags_in_object_schema() {
        let src = "const User = z.object({ id: z.string().uuid() });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_bare_z_string() {
        assert!(run_on("const s = z.string();").is_empty());
    }
}

//! OxcCheck backend for zod-prefer-strict-object.

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

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // callee must be `<receiver>.strict`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "strict" {
            return;
        }

        // The receiver must be a call expression whose callee is `z.object`.
        let Expression::CallExpression(receiver_call) = &member.object else { return };
        let Expression::StaticMemberExpression(recv_member) = &receiver_call.callee else { return };
        if recv_member.property.name.as_str() != "object" {
            return;
        }
        let Expression::Identifier(obj_id) = &recv_member.object else { return };
        if obj_id.name.as_str() != "z" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.object({...}).strict()` is deprecated in Zod v4 — \
                      use `z.strictObject({...})` instead."
                .into(),
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
    fn flags_object_strict_chain() {
        assert_eq!(
            run("const S = z.object({ a: z.string() }).strict();").len(),
            1
        );
    }


    #[test]
    fn allows_strict_object_factory() {
        assert!(run("const S = z.strictObject({ a: z.string() });").is_empty());
    }


    #[test]
    fn ignores_bare_object() {
        assert!(run("const S = z.object({ a: z.string() });").is_empty());
    }
}

//! zod-prefer-loose-object oxc backend — flag `.passthrough()` chained after
//! `z.object(...)`.

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
        Some(&["passthrough"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `.passthrough` on a receiver.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "passthrough" {
            return;
        }

        // The receiver (object) must be a call to `z.object`.
        let Expression::CallExpression(receiver_call) = &member.object else { return };
        let Expression::StaticMemberExpression(recv_member) = &receiver_call.callee else {
            return;
        };
        let Expression::Identifier(base) = &recv_member.object else { return };
        if base.name.as_str() != "z" || recv_member.property.name.as_str() != "object" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.object({...}).passthrough()` is deprecated in Zod v4 — \
                      use `z.looseObject({...})` instead."
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
    fn flags_passthrough_chain() {
        assert_eq!(
            run("const S = z.object({ a: z.string() }).passthrough();").len(),
            1
        );
    }


    #[test]
    fn allows_loose_object_factory() {
        assert!(run("const S = z.looseObject({ a: z.string() });").is_empty());
    }


    #[test]
    fn ignores_bare_object() {
        assert!(run("const S = z.object({ a: z.string() });").is_empty());
    }
}

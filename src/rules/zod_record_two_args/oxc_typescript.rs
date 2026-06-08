//! OxcCheck backend for zod-record-two-args.

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
        Some(&["record"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "record" {
            return;
        }
        let Expression::Identifier(obj_id) = &member.object else { return };
        if obj_id.name.as_str() != "z" {
            return;
        }

        if call.arguments.len() != 1 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.record(valueSchema)` with a single argument is removed in Zod v4 — \
                      pass the key schema explicitly: `z.record(z.string(), valueSchema)`."
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
    fn flags_single_arg_record() {
        assert_eq!(run("const S = z.record(z.string());").len(), 1);
    }


    #[test]
    fn allows_two_arg_record() {
        assert!(run("const S = z.record(z.string(), z.number());").is_empty());
    }


    #[test]
    fn ignores_unrelated_record_call() {
        assert!(run("const S = foo.record(x);").is_empty());
    }
}

//! require-to-throw-message — OXC backend.

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
        Some(&["toThrow"])
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
        let name = member.property.name.as_str();
        if name != "toThrow" && name != "toThrowError" {
            return;
        }

        // Skip `.not.toThrow()` / `.not.toThrowError()` — asserts no error; no argument needed
        if let Expression::StaticMemberExpression(obj_member) = &member.object {
            if obj_member.property.name.as_str() == "not" {
                return;
            }
        }

        // Flag only when called with zero arguments.
        if !call.arguments.is_empty() {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Provide expected error message to toThrow().".into(),
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
    fn flags_empty_to_throw() {
        let d = run_on("expect(() => foo()).toThrow();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "require-to-throw-message");
    }


    #[test]
    fn flags_empty_to_throw_error() {
        let d = run_on("expect(() => foo()).toThrowError();");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_to_throw_with_string() {
        assert!(run_on("expect(() => foo()).toThrow('boom');").is_empty());
    }


    #[test]
    fn allows_to_throw_error_with_regex() {
        assert!(run_on("expect(() => foo()).toThrowError(/boom/);").is_empty());
    }


    #[test]
    fn ignores_unrelated_member_calls() {
        assert!(run_on("expect(x).toBe();").is_empty());
    }


    #[test]
    fn no_fp_on_not_to_throw() {
        // .not.toThrow() asserts no error is thrown — no argument needed (Closes #440)
        assert!(run_on("expect(() => fn()).not.toThrow();").is_empty());
    }


    #[test]
    fn no_fp_on_not_to_throw_error() {
        assert!(run_on("expect(() => fn()).not.toThrowError();").is_empty());
    }
}

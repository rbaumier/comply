//! test-check-exception OXC backend — flag `.toThrow()` with no arguments in test files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

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
        if !is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Callee must be a member expression with property "toThrow"
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "toThrow" {
            return;
        }
        // Skip `.not.toThrow()` — asserts no error is thrown; no argument needed or meaningful
        if let oxc_ast::ast::Expression::StaticMemberExpression(obj_member) = &member.object {
            if obj_member.property.name.as_str() == "not" {
                return;
            }
        }
        // Arguments must be empty
        if !call.arguments.is_empty() {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.toThrow()` without specifying error type or message — any error will pass."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

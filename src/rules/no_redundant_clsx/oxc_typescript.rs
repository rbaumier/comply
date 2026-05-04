//! OXC backend for no-redundant-clsx — flag `clsx("foo")` / `cn("foo")` calls
//! with a single static string argument.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const NAMES: &[&str] = &["clsx", "cn"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["clsx", "cn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let name = callee.name.as_str();
        if !NAMES.contains(&name) {
            return;
        }

        // Exactly one argument, and it must be a string literal.
        if call.arguments.len() != 1 {
            return;
        }
        let Some(arg) = call.arguments.first() else { return };
        if !matches!(arg, oxc_ast::ast::Argument::StringLiteral(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}()` with a single static string is redundant — use the string directly.",
                name
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

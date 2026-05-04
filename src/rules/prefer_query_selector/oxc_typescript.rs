//! prefer-query-selector oxc backend — flag legacy DOM query methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const METHODS: &[(&str, &str)] = &[
    ("getElementById", "querySelector"),
    ("getElementsByClassName", "querySelectorAll"),
    ("getElementsByTagName", "querySelectorAll"),
    ("getElementsByName", "querySelectorAll"),
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["getElementById", "getElementsByClassName", "getElementsByTagName", "getElementsByName"])
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
        let method_name = member.property.name.as_str();

        let Some((_, replacement)) = METHODS.iter().find(|(m, _)| *m == method_name) else { return };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Prefer `.{replacement}()` over `.{method_name}()`."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

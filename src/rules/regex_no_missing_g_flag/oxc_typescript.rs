//! OxcCheck backend for regex-no-missing-g-flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

/// Methods whose regex argument must carry the `g` flag for correctness.
const G_REQUIRED_METHODS: &[&str] = &["matchAll", "replaceAll"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["matchAll", "replaceAll"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `<expr>.matchAll(...)` or `<expr>.replaceAll(...)`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();
        if !G_REQUIRED_METHODS.contains(&method) {
            return;
        }

        // First argument must be a regex literal without the `g` flag
        let Some(first_arg) = call.arguments.first() else { return };
        let Some(Expression::RegExpLiteral(re)) = first_arg.as_expression() else { return };
        if re.regex.flags.contains(oxc_ast::ast::RegExpFlags::G) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Regex passed to a method that requires the `g` flag but it is missing.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

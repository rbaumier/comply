//! OxcCheck backend for rn-no-inline-styles — flag `style={{ ... }}` on JSX elements.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("react-native") {
            return;
        }
        let AstKind::JSXAttribute(attr) = node.kind() else { return };
        if attr.name.as_identifier().map_or(true, |id| id.name.as_str() != "style") {
            return;
        }
        let Some(value) = &attr.value else { return };
        let oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container) = value else {
            return;
        };
        let oxc_ast::ast::JSXExpression::ObjectExpression(obj) = &container.expression else {
            return;
        };
        let span = obj.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Inline style object allocates on every render — use `StyleSheet.create` or `useMemo`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

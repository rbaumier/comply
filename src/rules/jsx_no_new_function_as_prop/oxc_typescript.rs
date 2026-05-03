use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXExpression;
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
        let AstKind::JSXAttribute(attr) = node.kind() else { return };
        let oxc_ast::ast::JSXAttributeName::Identifier(name_ident) = &attr.name else { return };
        let attr_name = name_ident.name.as_str();

        let Some(oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container)) = &attr.value
        else {
            return;
        };

        let kind_label = match &container.expression {
            JSXExpression::ArrowFunctionExpression(_) => "arrow function",
            JSXExpression::FunctionExpression(_) => "function expression",
            _ => return,
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, container.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{kind_label} as value of JSX prop `{attr_name}` creates a new reference every render — hoist with `useCallback` or to a stable handler."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

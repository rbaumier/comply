//! OxcCheck backend for tanstack-query-prefer-key-factory.
//!
//! Flag `queryKey: ['name', dynamicArg]` — a string prefix followed by a
//! variable element.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryKey"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        // Check key is `queryKey`
        let key_name = prop.key.static_name();
        let Some(ref name) = key_name else { return };
        if name.as_ref() != "queryKey" {
            return;
        }

        // Value must be an array expression
        let Expression::ArrayExpression(arr) = &prop.value else { return };

        let mut has_string = false;
        let mut has_variable = false;
        for elem in &arr.elements {
            let oxc_ast::ast::ArrayExpressionElement::SpreadElement(_) = elem else {
                match elem {
                    oxc_ast::ast::ArrayExpressionElement::StringLiteral(_)
                    | oxc_ast::ast::ArrayExpressionElement::TemplateLiteral(_) => {
                        has_string = true;
                    }
                    oxc_ast::ast::ArrayExpressionElement::NumericLiteral(_)
                    | oxc_ast::ast::ArrayExpressionElement::BooleanLiteral(_)
                    | oxc_ast::ast::ArrayExpressionElement::NullLiteral(_) => {}
                    oxc_ast::ast::ArrayExpressionElement::Elision(_) => {}
                    _ => {
                        has_variable = true;
                    }
                }
                continue;
            };
            has_variable = true;
        }

        if !(has_string && has_variable) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Extract dynamic `queryKey` to a key factory: `const keys = { detail: (id) => ['res', id] as const }`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

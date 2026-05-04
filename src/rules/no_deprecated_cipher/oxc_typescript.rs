//! OxcCheck backend — flag `createCipher()` calls (but not `createCipheriv()`).

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
        Some(&["createCipher"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let method_name = match &call.callee {
            Expression::StaticMemberExpression(member) => {
                Some(member.property.name.as_str())
            }
            Expression::Identifier(ident) => {
                Some(ident.name.as_str())
            }
            _ => None,
        };

        let Some(name) = method_name else { return };

        // Match exactly "createCipher" but NOT "createCipheriv" etc.
        if name != "createCipher" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`createCipher()` is deprecated — use `createCipheriv()` with an explicit IV.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

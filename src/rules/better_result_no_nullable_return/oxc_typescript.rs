//! better-result-no-nullable-return oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

fn imports_better_result(source: &str) -> bool {
    source.contains("better-result") || source.contains("@better-result")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["better-result"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !imports_better_result(ctx.source) {
            return;
        }
        let ret_annotation = match node.kind() {
            AstKind::Function(func) => func.return_type.as_ref(),
            AstKind::ArrowFunctionExpression(arrow) => arrow.return_type.as_ref(),
            _ => return,
        };
        let Some(ret) = ret_annotation else { return };
        let span = ret.span();
        let text = &ctx.source[span.start as usize..span.end as usize];
        let has_nullable = text.contains("| null")
            || text.contains("|null")
            || text.contains("| undefined")
            || text.contains("|undefined");
        if !has_nullable {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Replace nullable return type with Result<T, NotFoundError> in better-result modules.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

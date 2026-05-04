use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_functions_file(ctx: &CheckCtx) -> bool {
    let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    file_name.ends_with(".functions.ts") || file_name.ends_with(".functions.tsx")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["createServerFn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if is_functions_file(ctx) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::Identifier(id) = &call.callee else { return };
        if id.name.as_str() != "createServerFn" {
            return;
        }
        let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`createServerFn` must be in a `*.functions.ts` file, not `{file_name}`."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

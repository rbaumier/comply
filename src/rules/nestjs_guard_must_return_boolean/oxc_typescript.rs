use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::PropertyKey;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "CanActivate")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MethodDefinition]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_nestjs_file(ctx.source) {
            return;
        }
        let AstKind::MethodDefinition(method) = node.kind() else {
            return;
        };
        let name = match &method.key {
            PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
            _ => return,
        };
        if name != "canActivate" {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, method.key.span().start as usize);
        let Some(ret_type) = &method.value.return_type else {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`canActivate` is missing an explicit return type — must be `boolean | Promise<boolean> | Observable<boolean>`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        };
        let rt_text = &ctx.source[ret_type.span.start as usize..ret_type.span.end as usize];
        if rt_text.contains("boolean") {
            return;
        }
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`canActivate` return type `{rt_text}` should resolve to `boolean`."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

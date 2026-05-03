//! nestjs-no-sync-controller-method OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::PropertyKey;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_nestjs_file(source: &str) -> bool {
    source.contains("@nestjs/") || source.contains("@Controller")
}

const ROUTE_DECORATORS: &[&str] = &[
    "@Get", "@Post", "@Put", "@Patch", "@Delete", "@All", "@Options", "@Head",
];

fn method_has_route_decorator(
    method: &oxc_ast::ast::MethodDefinition,
    source: &str,
) -> Option<String> {
    for dec in &method.decorators {
        let dec_text = &source[dec.span.start as usize..dec.span.end as usize];
        if ROUTE_DECORATORS.iter().any(|d| dec_text.starts_with(d)) {
            return Some(dec_text.to_string());
        }
    }
    None
}

fn return_type_is_promise(method: &oxc_ast::ast::MethodDefinition, source: &str) -> bool {
    if let Some(ret_type) = &method.value.return_type {
        let text = &source[ret_type.span.start as usize..ret_type.span.end as usize];
        return text.contains("Promise<") || text.contains("Observable<");
    }
    false
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
        let AstKind::MethodDefinition(method) = node.kind() else { return };

        let Some(deco) = method_has_route_decorator(method, ctx.source) else {
            return;
        };

        if method.value.r#async {
            return;
        }
        if return_type_is_promise(method, ctx.source) {
            return;
        }

        let name = match &method.key {
            PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
            _ => return,
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, method.key.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Controller method `{name}` ({deco}) should be `async` or return a `Promise`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

//! OxcCheck backend for better-auth-middleware-requires-headers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use std::sync::Arc;

pub struct Check;

fn is_middleware_file(ctx: &CheckCtx) -> bool {
    ctx.path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "middleware.ts" || n == "middleware.tsx" || n == "middleware.js")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["getSession"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_middleware_file(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "getSession" {
            return;
        }

        // Check if first arg is an object with a `headers` key.
        let has_headers = call.arguments.first().is_some_and(|arg| {
            let Some(Expression::ObjectExpression(obj)) = arg.as_expression() else { return false };
            obj.properties.iter().any(|prop| {
                let ObjectPropertyKind::ObjectProperty(p) = prop else { return false };
                p.key.static_name().is_some_and(|n| n == "headers")
            })
        });

        if has_headers {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`getSession()` in middleware must forward request headers — pass `{ headers: await headers() }` or session lookup will fail.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{Diagnostic, Severity};
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;
    use super::Check;

}

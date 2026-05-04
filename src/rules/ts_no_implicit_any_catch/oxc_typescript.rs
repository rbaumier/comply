//! ts-no-implicit-any-catch OXC backend — flag `catch (e)` without a type annotation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };
        let Some(handler) = &try_stmt.handler else { return };
        let Some(param) = &handler.param else {
            // `catch { ... }` — no binding, nothing to annotate.
            return;
        };
        // If the catch parameter has a type annotation, it's fine.
        if param.type_annotation.is_some() {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, param.pattern.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "catch binding has no type annotation — it defaults to `any`. \
                      Use `catch (e: unknown)` and narrow the value explicitly."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

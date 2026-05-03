use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };

        let Some(handler) = &try_stmt.handler else { return };
        let Some(param) = &handler.param else {
            return; // bare `catch { ... }` — nothing to flag.
        };
        // Only handle simple identifier bindings.
        let BindingPattern::BindingIdentifier(ident) = &param.pattern else {
            return;
        };
        let name = ident.name.as_str();

        // Use semantic symbol info to check if the binding is referenced.
        if let Some(symbol_id) = ident.symbol_id.get() {
            let mut refs = semantic.symbol_references(symbol_id);
            if refs.next().is_some() {
                return; // has at least one reference — binding is used.
            }
        } else {
            // No symbol id — fallback to text check in the body.
            let body_src =
                &ctx.source[handler.body.span.start as usize..handler.body.span.end as usize];
            if body_src.contains(name) {
                return;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, ident.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`catch ({name})` is never used — drop the binding (`catch {{ ... }}`) \
                 or reference `{name}` in the handler."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

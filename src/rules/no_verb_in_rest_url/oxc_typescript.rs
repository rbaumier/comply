//! no-verb-in-rest-url oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/api/", "/v1/", "/v2/"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (text, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => (lit.value.as_str().to_string(), lit.span.start as usize),
            AstKind::TemplateLiteral(tpl) => {
                let s: String = tpl
                    .quasis
                    .iter()
                    .map(|q| q.value.raw.as_str())
                    .collect::<Vec<_>>()
                    .join("");
                (s, tpl.span.start as usize)
            }
            _ => return,
        };
        let Some(verb) = super::verb_url_match::contains_verb_url(&text) else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "REST URL contains the verb '{verb}' — use HTTP semantics instead. \
                 `POST /api/orders` creates, `GET /api/orders/:id` reads, \
                 `PATCH /api/orders/:id` updates, `DELETE /api/orders/:id` removes."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

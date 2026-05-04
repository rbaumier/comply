//! hono-secure-headers-disabled OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::PropertyKey;
use oxc_span::GetSpan;
use std::sync::Arc;

const SECURITY_HEADERS: &[&str] = &[
    "strictTransportSecurity",
    "xFrameOptions",
    "xContentTypeOptions",
    "removePoweredBy",
    "referrerPolicy",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono/secure-headers"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.source.contains("hono/secure-headers") {
            return;
        }

        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        let key_text = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };

        if !SECURITY_HEADERS.contains(&key_text) {
            return;
        }

        // Check if value is `false`.
        let value_text = &ctx.source[prop.value.span().start as usize..prop.value.span().end as usize];
        if value_text.trim() != "false" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{}` is explicitly disabled — this removes a security protection.", key_text),
            severity: Severity::Error,
            span: None,
        });
    }
}

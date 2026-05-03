//! zod-brand-ids oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Return `true` if `key` is an ID-like field name.
fn is_id_like(key: &str) -> bool {
    let key = key.trim_matches(|c: char| c == '"' || c == '\'');
    if key.eq_ignore_ascii_case("id") {
        return true;
    }
    if key.strip_suffix("_id").is_some_and(|p| !p.is_empty()) {
        return true;
    }
    if key.strip_suffix("_ID").is_some_and(|p| !p.is_empty()) {
        return true;
    }
    for suffix in ["Id", "ID"] {
        if let Some(prefix) = key.strip_suffix(suffix) {
            if prefix.is_empty() {
                continue;
            }
            let last = prefix.chars().next_back().unwrap_or(' ');
            if last.is_ascii_lowercase() || last.is_ascii_digit() {
                return true;
            }
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };

        let key_text = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(lit) => lit.value.as_str(),
            _ => return,
        };
        if !is_id_like(key_text) {
            return;
        }

        let value_span = prop.value.span();
        let value_text = &ctx.source[value_span.start as usize..value_span.end as usize];
        if !value_text.starts_with("z.") {
            return;
        }
        if value_text.contains(".brand(") || value_text.contains(".brand<") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.key.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}` looks like an ID — add `.brand<\"...\">()` so distinct IDs \
                 are not assignable to each other.",
                key_text.trim_matches(|c: char| c == '"' || c == '\''),
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

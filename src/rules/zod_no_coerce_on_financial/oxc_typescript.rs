//! zod-no-coerce-on-financial oxc backend — flag `pair` nodes whose key is a
//! financial-sounding field and whose value starts with `z.coerce.`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::PropertyKey;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_financial_key(key: &str) -> bool {
    let k = key
        .trim_matches(|c: char| c == '"' || c == '\'')
        .to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "price", "amount", "money", "currency", "cost", "fee", "total", "subtotal", "balance",
        "salary", "wage",
    ];
    NEEDLES.iter().any(|n| k.contains(n))
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.coerce"])
    }

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
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if !is_financial_key(key_text) {
            return;
        }

        // Check if value text contains `z.coerce.`
        let value_span = prop.value.span();
        let value_text =
            &ctx.source[value_span.start as usize..value_span.end as usize];
        if !value_text.contains("z.coerce.") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.key.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}` is a financial field — `z.coerce.*` silently accepts invalid \
                 strings. Parse explicitly with a regex + `.transform(Number)`.",
                key_text.trim_matches(|c: char| c == '"' || c == '\''),
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

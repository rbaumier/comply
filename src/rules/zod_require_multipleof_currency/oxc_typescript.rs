//! zod-require-multipleof-currency — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::PropertyKey;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_currency_key(key: &str) -> bool {
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
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["multipleOf"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        let key_text = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if !is_currency_key(key_text) {
            return;
        }

        let value_text =
            &ctx.source[prop.value.span().start as usize..prop.value.span().end as usize];

        let is_number =
            value_text.contains("z.number(") || value_text.contains("z.coerce.number(");
        if !is_number {
            return;
        }

        if value_text.contains(".multipleOf(") || value_text.contains(".int(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.key.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}` is a currency field — add `.multipleOf(0.01)` (or use `.int()` \
                 minor units) to prevent sub-cent precision bugs.",
                key_text
                    .trim_matches(|c: char| c == '"' || c == '\''),
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_price_without_multipleof() {
        assert_eq!(run("const S = z.object({ price: z.number() });").len(), 1);
    }


    #[test]
    fn allows_multipleof() {
        assert!(run("const S = z.object({ price: z.number().multipleOf(0.01) });").is_empty());
    }


    #[test]
    fn allows_int_minor_units() {
        assert!(run("const S = z.object({ priceCents: z.number().int() });").is_empty());
    }


    #[test]
    fn ignores_non_currency_field() {
        assert!(run("const S = z.object({ age: z.number() });").is_empty());
    }
}

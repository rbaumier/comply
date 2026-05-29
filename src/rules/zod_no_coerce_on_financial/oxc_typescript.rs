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

        let value_span = prop.value.span();
        let value_text =
            &ctx.source[value_span.start as usize..value_span.end as usize];
        // Only flag numeric coercions — date/string/boolean coercions on a
        // field whose name contains a financial keyword are not financial risks.
        if !value_text.contains("z.coerce.number")
            && !value_text.contains("z.coerce.bigint")
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_coerce_number_on_price() {
        assert_eq!(
            run("const S = z.object({ price: z.coerce.number() });").len(),
            1
        );
    }

    #[test]
    fn flags_coerce_number_on_amount() {
        assert_eq!(
            run("const S = z.object({ amount: z.coerce.number() });").len(),
            1
        );
    }

    #[test]
    fn no_fp_on_price_updated_at_with_coerce_date() {
        // Regression for #331: priceUpdatedAt is a timestamp, not a financial amount.
        assert!(
            run("const S = z.object({ priceUpdatedAt: z.coerce.date().nullable() });").is_empty()
        );
    }

    #[test]
    fn no_fp_on_coerce_date_with_financial_name() {
        assert!(
            run("const S = z.object({ feeDate: z.coerce.date() });").is_empty()
        );
    }

    #[test]
    fn allows_explicit_parse() {
        assert!(
            run(r#"const S = z.object({ price: z.string().regex(/^\d+$/).transform(Number) });"#)
                .is_empty()
        );
    }

    #[test]
    fn ignores_non_financial_field() {
        assert!(run("const S = z.object({ count: z.coerce.number() });").is_empty());
    }
}

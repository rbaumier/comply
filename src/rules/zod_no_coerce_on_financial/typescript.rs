//! zod-no-coerce-on-financial backend — flag `pair` nodes whose key is a
//! financial-sounding field and whose value starts with `z.coerce.`.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { on ["pair"] prefilter = ["z.coerce"] => |node, source, ctx, diagnostics|
    let Some(key_node) = node.child_by_field_name("key") else { return };
    let Some(value_node) = node.child_by_field_name("value") else { return };

    let Ok(key_text) = key_node.utf8_text(source) else { return };
    if !is_financial_key(key_text) { return; }

    let Ok(value_text) = value_node.utf8_text(source) else { return };
    // Only flag numeric coercions — date/string/boolean coercions on a
    // field whose name contains a financial keyword are not financial risks.
    if !value_text.contains("z.coerce.number") && !value_text.contains("z.coerce.bigint") { return; }

    let pos = key_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_coerce_on_price() {
        assert_eq!(
            run("const S = z.object({ price: z.coerce.number() });").len(),
            1
        );
    }

    #[test]
    fn flags_coerce_on_amount() {
        assert_eq!(
            run("const S = z.object({ amount: z.coerce.number() });").len(),
            1
        );
    }

    #[test]
    fn allows_explicit_parse() {
        assert!(
            run("const S = z.object({ price: z.string().regex(/^\\d+$/).transform(Number) });")
                .is_empty()
        );
    }

    #[test]
    fn ignores_non_financial_field() {
        assert!(run("const S = z.object({ count: z.coerce.number() });").is_empty());
    }

    #[test]
    fn no_fp_on_price_updated_at_with_coerce_date() {
        // Regression for #331: priceUpdatedAt is a timestamp, not a financial amount.
        assert!(
            run("const S = z.object({ priceUpdatedAt: z.coerce.date().nullable() });").is_empty()
        );
    }
}

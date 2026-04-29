//! zod-require-multipleof-currency backend — flag number schemas on currency
//! fields that don't chain `.multipleOf` or declare `.int()`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_currency_key(key: &str) -> bool {
    let k = key.trim_matches(|c: char| c == '"' || c == '\'').to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "price", "amount", "money", "currency", "cost", "fee",
        "total", "subtotal", "balance", "salary", "wage",
    ];
    NEEDLES.iter().any(|n| k.contains(n))
}

crate::ast_check! { on ["pair"] prefilter = ["multipleOf"] => |node, source, ctx, diagnostics|
    let Some(key_node) = node.child_by_field_name("key") else { return };
    let Some(value_node) = node.child_by_field_name("value") else { return };

    let Ok(key_text) = key_node.utf8_text(source) else { return };
    if !is_currency_key(key_text) { return; }

    let Ok(value_text) = value_node.utf8_text(source) else { return };
    // Only care about number schemas (coerce.number or z.number).
    let is_number = value_text.contains("z.number(") || value_text.contains("z.coerce.number(");
    if !is_number { return; }

    // Ok if already constrained to cents or integer minor units.
    if value_text.contains(".multipleOf(") || value_text.contains(".int(") {
        return;
    }

    let pos = key_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`{}` is a currency field — add `.multipleOf(0.01)` (or use `.int()` \
             minor units) to prevent sub-cent precision bugs.",
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

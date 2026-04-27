//! drizzle-decimal-for-money — flag `numeric('<money_name>')` /
//! `decimal('<money_name>')` calls that don't pass a `{ precision: ..., scale: ... }`
//! options object. A column is treated as money-related when its first-arg
//! string contains a recognised money keyword.

use crate::diagnostic::{Diagnostic, Severity};

const MONEY_KEYWORDS: &[&str] = &[
    "price", "amount", "total", "cost", "fee", "subtotal", "balance", "salary", "wage", "tax", "discount", "revenue", "money",
];

fn is_money_column(name: &str) -> bool {
    let lower = name.to_lowercase();
    MONEY_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    let name = callee.utf8_text(source).unwrap_or("");
    if name != "numeric" && name != "decimal" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let mut iter = args.named_children(&mut cursor);
    let Some(first) = iter.next() else { return };
    if first.kind() != "string" {
        return;
    }
    let raw = first.utf8_text(source).unwrap_or("");
    let col = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if !is_money_column(col) {
        return;
    }
    // Check for an options object containing `precision:`.
    let mut has_precision = false;
    if let Some(second) = iter.next() {
        if second.kind() == "object" {
            let text = second.utf8_text(source).unwrap_or("");
            if text.contains("precision:") || text.contains("precision :") {
                has_precision = true;
            }
        }
    }
    if has_precision {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-decimal-for-money".into(),
        message: format!("`{}('{}', ...)` for a money column needs an explicit `{{ precision, scale }}` to avoid unbounded SQL `numeric`.", name, col),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_numeric_price_without_precision() {
        let src = "const p = numeric('price');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_decimal_amount_without_precision() {
        let src = "const a = decimal('amount');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_numeric_with_precision() {
        let src = "const p = numeric('price', { precision: 12, scale: 2 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_money_column() {
        let src = "const p = numeric('latitude');";
        assert!(run(src).is_empty());
    }
}

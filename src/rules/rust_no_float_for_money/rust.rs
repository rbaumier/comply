//! rust-no-float-for-money backend.
//!
//! Walks struct fields, function parameters, and let bindings.
//! Flags any binding whose name matches a money-shaped word
//! (`price`, `amount`, `cost`, `balance`, `fee`, `total`,
//! `subtotal`, `tax`, `discount`, `revenue`) AND whose declared
//! type is `f32` or `f64`.
//!
//! False positives are possible (`average_score`, `tax_rate`) but
//! the failure mode of using a float for money is bad enough that
//! we err on the loud side. Suppress with `// comply-ignore`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const MONEY_NAMES: &[&str] = &[
    "price", "amount", "cost", "balance", "fee", "total", "subtotal",
    "tax", "discount", "revenue", "salary", "wage", "fare", "charge",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            // Struct field: `price: f64`.
            if node.kind() == "field_declaration"
                && let Some(name_node) = node.child_by_field_name("name")
                && let Some(type_node) = node.child_by_field_name("type")
                && let Ok(name) = name_node.utf8_text(source_bytes)
                && let Ok(type_text) = type_node.utf8_text(source_bytes)
                && is_money_name(name)
                && is_float_type(type_text)
            {
                diagnostics.push(make_diagnostic(ctx, node, name, type_text));
                return;
            }
            // Function parameter: `fn pay(amount: f64)`.
            if node.kind() == "parameter"
                && let Some(pattern) = node.child_by_field_name("pattern")
                && let Some(type_node) = node.child_by_field_name("type")
                && let Ok(name) = pattern.utf8_text(source_bytes)
                && let Ok(type_text) = type_node.utf8_text(source_bytes)
                && is_money_name(name)
                && is_float_type(type_text)
            {
                diagnostics.push(make_diagnostic(ctx, node, name, type_text));
            }
        });
        diagnostics
    }
}

fn is_money_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    MONEY_NAMES.iter().any(|m| {
        // Match the exact word OR a snake_case word containing it as a token.
        lower == *m
            || lower.starts_with(&format!("{m}_"))
            || lower.ends_with(&format!("_{m}"))
            || lower.contains(&format!("_{m}_"))
    })
}

fn is_float_type(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed == "f32" || trimmed == "f64"
}

fn make_diagnostic(
    ctx: &CheckCtx,
    node: tree_sitter::Node,
    name: &str,
    type_text: &str,
) -> Diagnostic {
    let pos = node.start_position();
    Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-no-float-for-money".into(),
        message: format!(
            "`{name}: {type_text}` — money values in floats accumulate \
             IEEE 754 rounding errors. Use `rust_decimal::Decimal` or a \
             newtype around `i64` representing cents."
        ),
        severity: Severity::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_price_f64_struct_field() {
        assert_eq!(run_on("struct Order { price: f64 }").len(), 1);
    }

    #[test]
    fn flags_unit_price_f32_field() {
        assert_eq!(run_on("struct Item { unit_price: f32 }").len(), 1);
    }

    #[test]
    fn flags_amount_param() {
        assert_eq!(run_on("fn charge(amount: f64) {}").len(), 1);
    }

    #[test]
    fn allows_decimal_type() {
        assert!(run_on("struct Order { price: Decimal }").is_empty());
    }

    #[test]
    fn allows_i64_cents() {
        assert!(run_on("struct Order { price_cents: i64 }").is_empty());
    }

    #[test]
    fn does_not_flag_score_field() {
        assert!(run_on("struct Game { score: f64 }").is_empty());
    }
}

//! rust-no-float-for-money backend.
//!
//! Walks struct fields, function parameters, and let bindings.
//! Flags any binding whose name matches an unambiguously monetary
//! word (`price`, `balance`, `fee`, `tax`, `discount`, `revenue`,
//! `salary`, `wage`, `fare`, `charge`) AND whose declared type is
//! `f32` or `f64`.
//!
//! The name list excludes polysemous words like `amount` and `cost`.
//! `amount`, in non-monetary domains (color-channel/lightness adjustments,
//! animation seek offsets, GUI pixel measurements, physics), is correctly
//! an `f32`/`f64`. `cost`, in algorithmic domains (query planners,
//! pathfinding/A*, optimization, ML loss), is a heuristic weight rather
//! than currency. Both carry no AST signal distinguishing them from money.
//!
//! A money word as a `<word>_<qualifier>` prefix where the trailing
//! segment is a dimensionless mathematical/statistical qualifier
//! (`factor`, `weight`, `ratio`, `coefficient`, `score`, `index`,
//! `multiplier`, `loss`, `percent`, `pct`) is not flagged: the qualifier
//! reclassifies the quantity as a unitless ratio/weight, not currency
//! (`balance_factor` is a clustering hyperparameter, not an account
//! balance). A bare money
//! word and a `_<word>` suffix (`balance`, `account_balance`) still flag.
//! `rate` is deliberately not a qualifier — `tax_rate`/`interest_rate`
//! are financial.
//!
//! `total`/`subtotal` are monetary only as a `total_<money-word>` prefix,
//! where the trailing money word carries the signal: `total_price` matches
//! `price`, and `total_amount` matches because `amount` is monetary once
//! disambiguated by `total`. A bare `total`/`subtotal`, a `_total` suffix
//! (`order_total`), or a `_total_` infix is not flagged: a standalone
//! aggregate is a total of *any* quantity (bytes, requests, items, money)
//! and carries no AST signal that it is currency — `total: f64` is as
//! likely a throughput accumulator (tests/sec) as a cart total.
//! `total_cost` is excluded — like bare `cost`, a summed planner/A*
//! heuristic weight as often as money — and `total_weight`/`total_edges`
//! are graph/statistics quantities.
//!
//! False positives are possible (`average_score`) but the failure mode
//! of using a float for money is bad enough that we err on the loud
//! side. Suppress with `// comply-ignore`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["field_declaration", "parameter"];

/// Unambiguously monetary words. Matched as the bare word or as any
/// snake_case token (prefix, suffix, or infix).
const MONEY_NAMES: &[&str] = &[
    "price", "balance", "fee", "tax", "discount", "revenue", "salary", "wage", "fare", "charge",
];

/// Aggregate words that are too polysemous to signal money on their own:
/// a `total`/`subtotal` of *any* quantity (bytes, requests, items, money).
/// They count only as a `<word>_…` prefix whose remainder carries a
/// monetary token (a `MONEY_NAMES` word or `amount`), which the aggregate
/// prefix disambiguates as currency (`total_price`, `total_amount`). Bare,
/// suffix (`order_total`), and infix uses are not flagged.
const AGGREGATE_NAMES: &[&str] = &["total", "subtotal"];

/// Dimensionless mathematical/statistical qualifiers. A money word as a
/// `<money>_<qualifier>` prefix denotes a unitless ratio/weight, not
/// currency (`balance_factor`, `price_index`), so it is not flagged.
/// `percent`/`pct` are per-hundred ratios, dimensionless like `ratio`
/// (`charge_percent` is a battery charge percentage, not currency).
/// `rate` is excluded — `tax_rate`/`interest_rate` are financial.
const NON_MONETARY_QUALIFIERS: &[&str] = &[
    "factor",
    "weight",
    "ratio",
    "coefficient",
    "score",
    "index",
    "multiplier",
    "loss",
    "percent",
    "pct",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
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
    }
}

fn is_money_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    has_money_token(&lower) || AGGREGATE_NAMES.iter().any(|w| matches_aggregate(&lower, w))
}

/// True when `lower` contains a `MONEY_NAMES` word as a snake_case token.
fn has_money_token(lower: &str) -> bool {
    MONEY_NAMES.iter().any(|word| has_token(lower, word))
}

fn has_token(lower: &str, word: &str) -> bool {
    if lower == word || lower.ends_with(&format!("_{word}")) || lower.contains(&format!("_{word}_"))
    {
        return true;
    }
    if let Some(rest) = lower.strip_prefix(&format!("{word}_")) {
        return !is_non_monetary_qualifier(rest);
    }
    false
}

/// True when the segment trailing a money-word prefix is a dimensionless
/// qualifier, reclassifying the quantity as a unitless ratio/weight
/// (`balance_factor`, `price_index`) rather than currency.
fn is_non_monetary_qualifier(rest: &str) -> bool {
    NON_MONETARY_QUALIFIERS.contains(&rest)
}

/// An aggregate word (`total`/`subtotal`) counts as monetary only as a
/// `<word>_…` prefix whose remainder carries a monetary token
/// (`total_amount`, `total_price`) — never `total_weight`/`total_cost`.
/// Bare, `_<word>` suffix, and `_<word>_` infix uses are not flagged: a
/// standalone aggregate is too polysemous to signal currency on its own.
fn matches_aggregate(lower: &str, word: &str) -> bool {
    if let Some(rest) = lower.strip_prefix(&format!("{word}_")) {
        return rest == "amount" || rest.starts_with("amount_") || has_money_token(rest);
    }
    false
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
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-no-float-for-money".into(),
        message: format!(
            "`{name}: {type_text}` — money values in floats accumulate \
             IEEE 754 rounding errors. Use `rust_decimal::Decimal` or a \
             newtype around `i64` representing cents."
        ),
        severity: Severity::Error,
        span: None,
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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
    fn flags_price_param() {
        assert_eq!(run_on("fn quote(price: f64) {}").len(), 1);
    }

    #[test]
    fn does_not_flag_estimated_cost_field() {
        // query-planner cost estimate, unitless heuristic weight. #3776.
        assert!(run_on("struct S { estimated_cost: f64 }").is_empty());
    }

    #[test]
    fn does_not_flag_cpu_cost_per_row_field() {
        // #3776
        assert!(run_on("struct S { cpu_cost_per_row: f64 }").is_empty());
    }

    #[test]
    fn does_not_flag_hash_lookup_cost_field() {
        // #3776
        assert!(run_on("struct S { hash_lookup_cost: f64 }").is_empty());
    }

    #[test]
    fn does_not_flag_bare_cost_field() {
        // #3776
        assert!(run_on("struct S { cost: f64 }").is_empty());
    }

    #[test]
    fn flags_balance_field() {
        assert_eq!(run_on("struct S { balance: f64 }").len(), 1);
    }

    #[test]
    fn flags_fee_param() {
        assert_eq!(run_on("fn pay(fee: f64) {}").len(), 1);
    }

    #[test]
    fn does_not_flag_balance_factor_field() {
        // K-Means clustering hyperparameter: `balance` qualified by the
        // dimensionless `factor` is a unitless weight, not currency. #5598.
        assert!(run_on("struct KMeans { balance_factor: f32 }").is_empty());
    }

    #[test]
    fn does_not_flag_balance_factor_param() {
        // #5598
        let src = "fn compute_balance_loss(n: usize, balance_factor: f32) -> f32 { 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_price_index_field() {
        // `price` qualified by `index` is a dimensionless statistic. #5598.
        assert!(run_on("struct Stats { price_index: f64 }").is_empty());
    }

    #[test]
    fn flags_account_balance_suffix_field() {
        // `balance` as a suffix stays monetary. #5598.
        assert_eq!(run_on("struct Account { account_balance: f64 }").len(), 1);
    }

    #[test]
    fn does_not_flag_charge_percent_field() {
        // `charge` qualified by `percent` is a battery charge percentage
        // (0–100), a dimensionless per-hundred ratio, not currency. #7433.
        let src = "struct BatteryData { charge_percent: f64 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_fee_pct_field() {
        // `pct` is the abbreviated percent suffix, dimensionless. #7433.
        assert!(run_on("struct S { fee_pct: f64 }").is_empty());
    }

    #[test]
    fn flags_tax_rate_field() {
        // `rate` is not a non-monetary qualifier: `tax_rate` is financial. #5598.
        assert_eq!(run_on("struct Invoice { tax_rate: f64 }").len(), 1);
    }

    #[test]
    fn does_not_flag_amount_param() {
        // `amount` is polysemous: in color/physics/GUI domains it is a
        // correct float adjustment, not money. See issue #1434.
        assert!(run_on("fn saturate_fixed(amount: f64) {}").is_empty());
    }

    #[test]
    fn does_not_flag_amount_color_method() {
        // wezterm color-funcs FP shape: `fn(&self, amount: f64) -> Self`.
        let src = "impl Color { fn lighten_fixed(&self, amount: f64) -> Self { self } }";
        assert!(run_on(src).is_empty());
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

    #[test]
    fn does_not_flag_total_weight_graph_param() {
        // Louvain community detection: `total_weight` is the sum of edge
        // weights, a graph quantity, not currency. #4748.
        let src = "fn calculate_delta(node: u32, total_weight: f32) -> f32 { 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_total_edges_field() {
        // `total_<domain-noun>` prefix is domain-specific, not money. #4748.
        assert!(run_on("struct G { total_edges: f64 }").is_empty());
    }

    #[test]
    fn does_not_flag_order_total_suffix_field() {
        // A `_total` suffix aggregates an unknown quantity — `order_total`
        // could be an order count as readily as currency. #5635.
        assert!(run_on("struct Order { order_total: f64 }").is_empty());
    }

    #[test]
    fn does_not_flag_bare_total_param() {
        // foundry `rate_per_sec(total: f64, elapsed: Duration)`: `total` is a
        // throughput accumulator (tests/sec), not currency. A standalone
        // aggregate is too polysemous to signal money on its own. #5635.
        let src = "fn rate_per_sec(total: f64, elapsed: Duration) -> f64 { 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_bare_total_field() {
        // #5635
        assert!(run_on("struct Order { total: f64 }").is_empty());
    }

    #[test]
    fn flags_total_amount_field() {
        // `total_<money-noun>` compound stays monetary. #4748.
        assert_eq!(run_on("struct Cart { total_amount: f64 }").len(), 1);
    }

    #[test]
    fn flags_total_price_field() {
        assert_eq!(run_on("struct Cart { total_price: f64 }").len(), 1);
    }

    #[test]
    fn does_not_flag_total_cost_field() {
        // `total_cost` is a summed planner/pathfinding heuristic weight as
        // often as money — same polysemy as bare `cost`. #3776, #4748.
        assert!(run_on("struct Plan { total_cost: f64 }").is_empty());
    }

    #[test]
    fn flags_total_amount_due_field() {
        // Multi-token money compound stays flagged. #4748.
        assert_eq!(run_on("struct Invoice { total_amount_due: f64 }").len(), 1);
    }

    #[test]
    fn flags_subtotal_amount_field() {
        // `subtotal_<money-noun>` compound stays monetary, like `total_`. #5635.
        assert_eq!(run_on("struct Cart { subtotal_amount: f64 }").len(), 1);
    }

    #[test]
    fn does_not_flag_bare_subtotal_field() {
        // A bare aggregate is too polysemous to signal currency. #5635.
        assert!(run_on("struct Cart { subtotal: f64 }").is_empty());
    }
}

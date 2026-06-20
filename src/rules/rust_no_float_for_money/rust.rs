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
//! `total`/`subtotal` are monetary only as the bare word, a suffix
//! (`order_total`, `cart_subtotal`), or a `_total_` infix — never as a
//! `total_<noun>` prefix, where the trailing noun decides the domain
//! (`total_weight`, `total_edges` are graph/statistics quantities).
//! A `total_<money-word>` compound stays flagged because the money word
//! carries the signal: `total_price` matches `price`, and `total_amount`
//! matches because `amount` is monetary once disambiguated by `total`.
//! `total_cost` is excluded — like bare `cost`, a summed planner/A*
//! heuristic weight as often as money.
//!
//! False positives are possible (`average_score`, `tax_rate`) but
//! the failure mode of using a float for money is bad enough that
//! we err on the loud side. Suppress with `// comply-ignore`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["field_declaration", "parameter"];

/// Unambiguously monetary words. Matched as the bare word or as any
/// snake_case token (prefix, suffix, or infix).
const MONEY_NAMES: &[&str] = &[
    "price", "balance", "fee", "tax", "discount", "revenue", "salary", "wage", "fare", "charge",
];

/// Aggregate words that are monetary on their own or as a suffix but
/// polysemous as a `<word>_<noun>` prefix (`total_weight`, `total_edges`
/// are graph quantities). Matched bare, as a `_<word>` suffix, or a
/// `_<word>_` infix. As a `<word>_…` prefix they only count when the
/// rest of the name carries a monetary token (a `MONEY_NAMES` word or
/// `amount`), which an aggregate prefix disambiguates from its otherwise
/// polysemous standalone use.
const AGGREGATE_NAMES: &[&str] = &["total", "subtotal"];

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
    lower == word
        || lower.starts_with(&format!("{word}_"))
        || lower.ends_with(&format!("_{word}"))
        || lower.contains(&format!("_{word}_"))
}

/// An aggregate word (`total`/`subtotal`) is monetary as the bare word,
/// a `_<word>` suffix, or a `_<word>_` infix. As a `<word>_…` prefix it
/// counts only when the rest of the name carries a monetary token
/// (`total_amount`, `total_price`) — never `total_weight`/`total_cost`.
fn matches_aggregate(lower: &str, word: &str) -> bool {
    if lower == word || lower.ends_with(&format!("_{word}")) || lower.contains(&format!("_{word}_"))
    {
        return true;
    }
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
    fn flags_order_total_suffix_field() {
        // `total` as a suffix stays monetary. #4748.
        assert_eq!(run_on("struct Order { order_total: f64 }").len(), 1);
    }

    #[test]
    fn flags_bare_total_field() {
        assert_eq!(run_on("struct Order { total: f64 }").len(), 1);
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
}

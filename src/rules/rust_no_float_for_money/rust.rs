//! rust-no-float-for-money backend.
//!
//! Walks struct fields, function parameters, and let bindings.
//! Flags any binding whose name matches an unambiguously monetary
//! word (`price`, `balance`, `fee`, `total`, `subtotal`,
//! `tax`, `discount`, `revenue`, `salary`, `wage`, `fare`, `charge`)
//! AND whose declared type is `f32` or `f64`.
//!
//! The name list excludes polysemous words like `amount` and `cost`.
//! `amount`, in non-monetary domains (color-channel/lightness adjustments,
//! animation seek offsets, GUI pixel measurements, physics), is correctly
//! an `f32`/`f64`. `cost`, in algorithmic domains (query planners,
//! pathfinding/A*, optimization, ML loss), is a heuristic weight rather
//! than currency. Both carry no AST signal distinguishing them from money.
//!
//! False positives are possible (`average_score`, `tax_rate`) but
//! the failure mode of using a float for money is bad enough that
//! we err on the loud side. Suppress with `// comply-ignore`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["field_declaration", "parameter"];

const MONEY_NAMES: &[&str] = &[
    "price", "balance", "fee", "total", "subtotal", "tax", "discount", "revenue", "salary", "wage",
    "fare", "charge",
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
}

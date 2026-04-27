//! sql-no-float-for-money — Drizzle ORM backend.
//!
//! Flags `real('col')` / `doublePrecision('col')` calls where the
//! column name hints at money. Floating-point arithmetic introduces
//! rounding errors that compound over transactions — use `numeric()`
//! instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const MONEY_KEYWORDS: &[&str] = &[
    "price", "amount", "money", "cost", "fee", "total", "balance",
    "salary", "revenue", "budget", "payment", "rate", "discount",
    "tax", "charge",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let Ok(name) = function.utf8_text(source_bytes) else {
            return;
        };
        if name != "real" && name != "doublePrecision" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        for i in 0..args.named_child_count() {
            let Some(arg) = args.named_child(i) else { continue };
            if arg.kind() == "string" {
                let Ok(raw) = arg.utf8_text(source_bytes) else { continue };
                let col_name = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
                let lower = col_name.to_ascii_lowercase();
                if MONEY_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}('{col_name}')` uses floating-point for a monetary \
                             column — use `numeric(precision, scale)` to avoid \
                             rounding errors that compound over transactions."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_real_price() {
        let src = "const price = real('price');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_double_precision_amount() {
        let src = "const amount = doublePrecision('total_amount');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_double_precision_latitude() {
        let src = "const lat = doublePrecision('latitude');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_numeric_price() {
        let src = "const price = numeric('price', { precision: 10, scale: 2 });";
        assert!(run(src).is_empty());
    }
}

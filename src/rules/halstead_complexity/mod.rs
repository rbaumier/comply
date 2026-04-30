//! halstead-complexity — flag functions whose Halstead metrics exceed
//! configured thresholds (volume, difficulty, effort).
//!
//! Halstead's software science counts distinct + total operators and
//! operands in a function body, then derives vocabulary / length /
//! volume / difficulty / effort / estimated bugs from those four counts.
//! A function that trips any one of the three configured ceilings is
//! reported once, with the metric that blew the budget.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "halstead-complexity",
    description: "Function Halstead volume/difficulty/effort exceeds threshold.",
    remediation: "Split the function into smaller helpers, reduce operator/operand churn, or extract repeated sub-expressions into named bindings.",
    severity: Severity::Warning,
    doc_url: Some("https://en.wikipedia.org/wiki/Halstead_complexity_measures"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

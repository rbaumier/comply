//! rust-float-eq-partial-cmp — `f32`/`f64` compared with `==`/`!=` lies
//! about precision. Rounding makes equal-on-paper values differ in the
//! lowest bits.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-float-eq-partial-cmp",
    description: "Don't compare floats with `==` / `!=`.",
    remediation: "Compare the absolute difference against an epsilon: \
                  `(a - b).abs() < f64::EPSILON` for tight tolerance, or \
                  a domain-specific epsilon for measurements. For ordering \
                  use `a.partial_cmp(&b)`. Float `==` matches bit patterns, \
                  not numerical equality, so `0.1 + 0.2 == 0.3` is false.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "correctness"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

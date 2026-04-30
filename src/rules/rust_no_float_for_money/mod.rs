//! rust-no-float-for-money — never store currency as f32/f64.
//!
//! IEEE 754 floats can't represent `0.1 + 0.2` exactly, so they
//! accumulate rounding error in any kind of currency arithmetic.
//! Use `rust_decimal::Decimal` for arbitrary-precision currency,
//! or a newtype around `i64` representing the smallest currency
//! unit (cents, satoshis, etc.).

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-float-for-money",
    description: "Money fields must not be `f32`/`f64` — IEEE 754 rounding errors corrupt totals.",
    remediation: "Use `rust_decimal::Decimal` for arbitrary-precision \
                  monetary values, or a newtype around `i64` representing \
                  the smallest unit (cents, satoshis, …). Floats accumulate \
                  rounding errors and silently break accounting invariants.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

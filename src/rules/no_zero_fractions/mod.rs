//! no-zero-fractions — flag `1.0`, `2.00` where the fractional part is
//! all zeros. TS/JS only: in Rust, `1.0` is idiomatic and required for
//! explicit f64 typing (`1.0` vs `1` = f64 vs i32).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-zero-fractions",
    description: "Disallow number literals with zero fractions or dangling dots.",
    remediation: "Remove the unnecessary `.0` fraction — write `1` instead of `1.0`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

//! prefer-math-min-max — flag comparison ternaries replaceable by Math.min/max.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-math-min-max",
    description: "Prefer `Math.min()`/`Math.max()` over comparison ternaries.",
    remediation: "Replace `value > max ? max : value` with `Math.min(value, max)` \
                  (or `Math.max` for the inverse pattern).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

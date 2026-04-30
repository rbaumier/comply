//! prefer-set-has — flag array `.includes()` inside loops -> use `Set#has()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-set-has",
    description: "Prefer `Set#has()` over `Array#includes()` when checking for existence or non-existence.",
    remediation: "Convert the array to a `Set` and use `.has()` instead of \
                  `.includes()`. `Array#includes()` is O(n) per call; \
                  `Set#has()` is O(1). This matters when the check is inside \
                  a loop or called repeatedly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

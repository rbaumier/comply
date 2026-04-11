//! no-in-misuse

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-in-misuse",
    description:
        "`in` operator on arrays checks keys (indices), not values — use `.includes()` instead.",
    remediation: "Replace `x in arr` with `arr.includes(x)` or use a `Set`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

//! arguments-order

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "arguments-order",
    description: "Function arguments appear to be in the wrong order.",
    remediation:
        "Swap the arguments so `expected` comes after `actual`, and `min` comes before `max`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

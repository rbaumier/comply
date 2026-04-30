//! index-of-compare-to-positive

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "index-of-compare-to-positive",
    description: "`.indexOf(…) > 0` misses index 0 — use `>= 0` or `!== -1`.",
    remediation: "Replace `> 0` with `>= 0` (or `!== -1`) to include the first element.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

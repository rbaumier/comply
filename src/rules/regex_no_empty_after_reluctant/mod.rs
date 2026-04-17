//! regex-no-empty-after-reluctant

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-after-reluctant",
    description: "Reluctant quantifier followed by end-of-pattern or group is useless.",
    remediation: "Remove the `?` from the quantifier — it has no effect when nothing follows it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

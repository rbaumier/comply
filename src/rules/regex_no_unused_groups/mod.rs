//! regex-no-unused-groups

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-unused-groups",
    description: "Named capturing group is defined but never referenced.",
    remediation: "Use the group via `.groups.name` or `$<name>` in a replacement, or convert to a non-capturing group `(?:...)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

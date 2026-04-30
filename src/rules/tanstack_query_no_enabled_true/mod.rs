mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-enabled-true",
    description: "`enabled: true` is the default in TanStack Query and should be omitted.",
    remediation: "Remove `enabled: true` — queries are enabled by default.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

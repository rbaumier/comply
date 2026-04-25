

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-prefer-query-options",
    description: "Inline `queryKey`/`queryFn` objects should be extracted to `queryOptions()` factories for reuse.",
    remediation: "Use `queryOptions({ queryKey: [...], queryFn: ... })` and import the factory where needed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

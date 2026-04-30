mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-fn-must-throw-on-error",
    description: "`queryFn` must throw on HTTP errors so TanStack Query can retry and surface them.",
    remediation: "Check `res.ok` and throw: `if (!res.ok) throw new Error(...)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

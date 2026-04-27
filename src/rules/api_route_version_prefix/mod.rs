mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-route-version-prefix",
    description: "API routes must start with a version prefix (/v1/, /v2/, …).",
    remediation:
        "Prefix the route path with a version segment, e.g. `/v1/users`. If routes are mounted on a versioned sub-router, disable this rule for that file.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

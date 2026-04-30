//! security-no-query-without-ownership

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-no-query-without-ownership",
    description: "DB lookups by primary key without an ownership filter (`userId`, `orgId`, `tenantId`) are IDOR vectors.",
    remediation: "Add an ownership filter (`where: { id, userId }`) or scope the query by the authenticated user/org.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

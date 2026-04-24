//! api-separate-input-output-types — flag type/interface declarations that
//! mix server-managed fields (`id`, `createdAt`, `updatedAt`) with fields
//! meant for client input. Such shapes tend to get reused as both request
//! input and response output, leaking implementation details into the
//! write path and forcing clients to send fields they shouldn't own.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-separate-input-output-types",
    description: "The same type must not serve as both request input and response output.",
    remediation:
        "Split into separate input (CreateXInput) and output (XResponse) types. Server-managed fields (id, createdAt, updatedAt) belong only in the output shape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

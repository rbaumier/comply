//! api-no-boolean-field-in-response — flag `boolean` properties in
//! interfaces/type aliases that name an API response shape.
//!
//! Booleans lock the contract into a two-state world. The moment a third
//! state appears (`pending`, `archived`, `scheduled`, ...), clients must
//! juggle two fields (`isActive` + `isArchived`) or the server breaks
//! existing consumers by flipping semantics. An enum / string-union
//! starts at two states and grows without breaking the wire format.
//!
//! Scope: only response-shaped names — `*Response`, `*Dto`, `*Payload`,
//! `*Reply`, `*Result`, `*Body`. Internal models and request types are
//! untouched.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-boolean-field-in-response",
    description:
        "Response types should use string-unions / enums instead of booleans for extensibility.",
    remediation:
        "Replace `isActive: boolean` with `status: 'active' | 'inactive' | ...` so new states don't break the wire contract.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

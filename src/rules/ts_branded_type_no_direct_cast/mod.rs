//! ts-branded-type-no-direct-cast — forbid `as BrandedType` outside validators.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-branded-type-no-direct-cast",
    description: "Branded types must be constructed through a validator function, not via a direct `as` cast.",
    remediation: "Route the value through the brand's dedicated validator (e.g. `parseUserId(x)`) which returns the branded type after checking invariants.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

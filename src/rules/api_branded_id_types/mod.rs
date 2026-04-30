//! api-branded-id-types — flag function parameters named `*Id` / `*_id`
//! typed as bare `string` or `number`. Branded types (e.g. `OrderId`)
//! prevent the "I passed the user id where the order id was expected"
//! class of bugs, which raw string/number parameters cannot catch.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-branded-id-types",
    description: "Entity IDs in public API signatures must use branded types, not raw string/number.",
    remediation: "Introduce a branded type such as `type OrderId = string & { readonly __brand: 'OrderId' }` and use it instead of `string`/`number` for ID parameters.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

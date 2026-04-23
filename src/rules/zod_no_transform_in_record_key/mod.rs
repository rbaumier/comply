//! zod-no-transform-in-record-key — using `.transform()` in a `z.record()`
//! key schema produces runtime keys Zod cannot round-trip: the transformed
//! output is used as the object key but the original input is gone, so
//! validation asymmetries and parse/serialize mismatches follow.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-transform-in-record-key",
    description: "`.transform()` inside a `z.record()` key schema mutates the object key after validation, causing parse/serialize asymmetry.",
    remediation: "Don't use transforms in record key schemas",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

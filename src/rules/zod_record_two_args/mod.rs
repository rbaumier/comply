//! zod-record-two-args — require `z.record(keySchema, valueSchema)` (Zod v4).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-record-two-args",
    description: "`z.record(valueSchema)` is removed in Zod v4 — the single-arg \
                  form leaves the key type implicit and makes migrations painful.",
    remediation: "Pass both arguments explicitly: `z.record(z.string(), valueSchema)`. \
                  Use a branded or enum key schema when you want a narrower key type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

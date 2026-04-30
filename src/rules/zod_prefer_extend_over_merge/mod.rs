//! zod-prefer-extend-over-merge — prefer `.extend(...)` over `.merge(...)` (Zod v4).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-extend-over-merge",
    description: "`.merge()` is removed in Zod v4 — `.extend()` is the canonical \
                  way to augment an object schema.",
    remediation: "Replace `A.merge(B)` with `A.extend(B.shape)` (or inline the fields).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

//! ts-no-non-null-asserted-nullish-coalescing — flag `x! ?? y`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-non-null-asserted-nullish-coalescing",
    description: "`x! ?? y` is contradictory — `!` asserts non-null, `??` handles null.",
    remediation: "Remove the `!` (let `??` do its job) or remove the `??` (if the value is never null).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

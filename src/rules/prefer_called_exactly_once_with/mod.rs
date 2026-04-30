//! prefer-called-exactly-once-with — collapse `toHaveBeenCalledTimes(1)` +
//! `toHaveBeenCalledWith(...)` into the single matcher
//! `toHaveBeenCalledExactlyOnceWith(...)`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-called-exactly-once-with",
    description: "Prefer `toHaveBeenCalledExactlyOnceWith(args)` over separate `toHaveBeenCalledTimes(1)` + `toHaveBeenCalledWith(args)` assertions.",
    remediation: "Use toHaveBeenCalledExactlyOnceWith(args) instead of separate assertions",
    severity: Severity::Warning,
    doc_url: Some("https://vitest.dev/api/expect.html#tohavebeencalledexactlyoncewith"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

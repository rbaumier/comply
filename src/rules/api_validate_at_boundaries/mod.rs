//! api-validate-at-boundaries — flag `.parse(...)` / `.safeParse(...)`
//! calls in functions that don't look like request handlers or
//! middleware. Validation should happen once at the system boundary;
//! re-validating in internal helpers duplicates schemas and implies the
//! typed contract between internal functions isn't trusted.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-validate-at-boundaries",
    description:
        "Validation schemas (zod.parse) must run only at API boundaries, not between internal typed functions.",
    remediation:
        "Move the `.parse(...)` call to the HTTP handler or middleware. Internal callers should trust the static type contract.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

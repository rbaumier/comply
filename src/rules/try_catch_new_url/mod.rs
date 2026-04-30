//! try-catch-new-url — flag `new URL(...)` outside a try block.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "try-catch-new-url",
    description: "`new URL(...)` can throw — wrap it in try/catch or use `URL.canParse`.",
    remediation: "`new URL(invalid)` throws a TypeError. Either wrap in try/catch \
                  and handle the invalid-URL case, or gate with `URL.canParse(s)` \
                  before constructing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["error-handling"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

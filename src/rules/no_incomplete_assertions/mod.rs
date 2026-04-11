//! no-incomplete-assertions

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-incomplete-assertions",
    description: "Assertion chain is missing the actual matcher.",
    remediation: "Complete the assertion with a matcher: `expect(x).toBe(...)`, `.toEqual(...)`, `.toThrow()`, etc. Bare `expect(x);` or `expect(x).not;` tests nothing.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

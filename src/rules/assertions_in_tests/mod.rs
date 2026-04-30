//! assertions-in-tests

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "assertions-in-tests",
    description: "Test functions must contain at least one assertion.",
    remediation: "Add `expect(...)`, `assert(...)`, `.should(...)`, `.toBe(...)`, `.toEqual(...)`, `.toMatch(...)`, or `.toThrow(...)` to the test body.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

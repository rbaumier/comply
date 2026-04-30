//! testing-no-and-in-test-name — flag " and " in `test` / `it` names.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-and-in-test-name",
    description: "Test names containing \" and \" usually test multiple behaviors — split into separate tests.",
    remediation: "Write one test per behavior; use `describe` to group related cases.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

//! testing-no-conditional-assertion — flag `expect(...)` calls inside an
//! `if`-statement body within a `test()` / `it()` callback.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-conditional-assertion",
    description: "Assertions inside if-branches silently skip when the branch is not taken — the test passes but checks nothing.",
    remediation: "Make the assertion unconditional. If the branch depends on input, split into separate tests or use expect.soft / describe.each.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

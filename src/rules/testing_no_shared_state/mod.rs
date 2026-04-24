//! testing-no-shared-state — flag top-level `let`/`var` in test files that
//! are mutated inside `test(...)` blocks without being reset in `beforeEach`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-shared-state",
    description: "Top-level let/var mutated across test() blocks without being reset in beforeEach — tests become order-dependent.",
    remediation: "Move the variable inside each test, or reset it in beforeEach(). Prefer fresh state per test over shared mutable state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

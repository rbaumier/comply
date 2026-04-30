//! testing-no-concurrent-without-context-expect — flag `test.concurrent`
//! callbacks that use the module-level `expect` instead of destructuring
//! `{ expect }` from the test context.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-concurrent-without-context-expect",
    description: "test.concurrent must destructure { expect } from the test context — the module-level expect is not scoped per concurrent test.",
    remediation: "Destructure expect from the test context: test.concurrent('...', ({ expect }) => { ... })",
    severity: Severity::Warning,
    doc_url: Some("https://vitest.dev/guide/test-context.html"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

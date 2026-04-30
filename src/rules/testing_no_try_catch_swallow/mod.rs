//! testing-no-try-catch-swallow — flag `try { ... } catch { }` inside a
//! `test()` / `it()` callback where the catch clause is empty.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-try-catch-swallow",
    description: "Empty catch around the act phase masks the very errors the test is meant to surface.",
    remediation: "Either let the error propagate, or assert on it with expect(() => fn()).toThrow(...) / expect(promise).rejects.toThrow(...).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

//! testing-no-undefined-mock-var — flag `jest.fn()` / `vi.fn()` mocks that are never configured.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-undefined-mock-var",
    description: "`jest.fn()` / `vi.fn()` stored in a variable but never configured with `mockReturnValue` / `mockResolvedValue` / `mockImplementation` always returns `undefined`.",
    remediation: "Configure the mock with `.mockReturnValue(...)`, `.mockResolvedValue(...)` or `.mockImplementation(...)`, or pass an implementation to `jest.fn(impl)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

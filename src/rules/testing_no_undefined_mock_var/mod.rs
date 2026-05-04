//! testing-no-undefined-mock-var — flag `jest.fn()` / `vi.fn()` mocks that are never configured.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

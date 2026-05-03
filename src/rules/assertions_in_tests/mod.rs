//! assertions-in-tests

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

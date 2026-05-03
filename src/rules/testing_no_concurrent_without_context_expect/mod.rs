//! testing-no-concurrent-without-context-expect — flag `test.concurrent`
//! callbacks that use the module-level `expect` instead of destructuring
//! `{ expect }` from the test context.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

//! testing-prefer-test-each — flag `for`/`forEach` loops that wrap `test` / `it`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;
use crate::rules::backend::Backend;

pub const META: RuleMeta = RuleMeta {
    id: "testing-prefer-test-each",
    description: "Looping over `test` / `it` hides failures — use `test.each` so each row is its own named case.",
    remediation: "Replace `for (const row of cases) { test(..., () => {...}) }` with `test.each(cases)(..., (row) => {...})`.",
    severity: Severity::Warning,
    doc_url: Some("https://jestjs.io/docs/api#testeachtablename-fn-timeout"),
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

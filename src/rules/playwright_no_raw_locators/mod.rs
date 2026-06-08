//! playwright-no-raw-locators — flag `page.locator()` with CSS selectors.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-raw-locators",
    description: "`page.locator('css-selector')` is brittle — prefer `getByRole`, `getByText`, etc.",
    remediation: "Replace `page.locator('.btn')` with \
                  `page.getByRole('button')` or `page.getByText('Submit')`. \
                  Semantic locators are resilient to markup changes and \
                  align with how users find elements on the page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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

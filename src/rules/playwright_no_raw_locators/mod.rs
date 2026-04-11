//! playwright-no-raw-locators — flag `page.locator()` with CSS selectors.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}

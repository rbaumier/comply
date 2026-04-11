//! no-page-click-deprecated — reject deprecated `page.click()` in Playwright.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-page-click-deprecated",
    description: "`page.click(selector)` is deprecated — use `page.locator(selector).click()`.",
    remediation: "Replace `page.click(selector)` with \
                  `page.locator(selector).click()`. The locator API \
                  auto-waits and auto-retries, and the direct page methods \
                  are deprecated in Playwright.",
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

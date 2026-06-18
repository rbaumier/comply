//! no-page-click-deprecated — reject deprecated `page.click()` in Playwright.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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

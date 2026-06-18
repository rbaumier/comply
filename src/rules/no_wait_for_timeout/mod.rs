//! no-wait-for-timeout — reject `waitForTimeout` in Playwright tests.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-wait-for-timeout",
    description: "`waitForTimeout` is a flaky sleep — wait for network or UI state instead.",
    remediation: "Replace `await page.waitForTimeout(ms)` with a web-first \
                  assertion like `await expect(locator).toBeVisible()` or \
                  `await page.waitForResponse(url)`. Fixed sleeps cause \
                  flaky tests on slow CI and waste time on fast machines.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}

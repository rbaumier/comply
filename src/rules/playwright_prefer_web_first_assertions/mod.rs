//! playwright-prefer-web-first-assertions — flag manual boolean assertions on locator methods.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-web-first-assertions",
    description: "`expect(await locator.isVisible()).toBe(true)` does not auto-retry — use web-first assertions.",
    remediation: "Replace `expect(await el.isVisible()).toBe(true)` with \
                  `await expect(el).toBeVisible()`. Web-first assertions \
                  auto-retry until the condition is met or the timeout \
                  expires, making tests more reliable.",
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

//! playwright-prefer-web-first-assertions — flag manual boolean assertions on locator methods.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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

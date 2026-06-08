//! playwright-no-networkidle — flag `"networkidle"` wait strategy.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-networkidle",
    description: "`networkidle` is fragile — it waits for no network activity for 500 ms, which is race-prone.",
    remediation: "Replace `networkidle` with a web-first assertion like \
                  `await expect(locator).toBeVisible()` or wait for a \
                  specific response with `page.waitForResponse()`. The \
                  `networkidle` strategy is timing-based and fails on \
                  pages with polling, analytics, or websockets.",
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

//! playwright-no-wait-for-timeout — disallow `page.waitForTimeout()`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-wait-for-timeout",
    description: "`page.waitForTimeout()` introduces fragile fixed sleeps in tests.",
    remediation: "Use web-first assertions or waitFor* with conditions instead of arbitrary timeouts",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-wait-for-timeout.md",
    ),
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

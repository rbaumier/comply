//! playwright-prefer-to-have-count — use web-first `toHaveCount` instead of `await locator.count()`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-to-have-count",
    description: "Prefer `expect(locator).toHaveCount(n)` over `expect(await locator.count()).toBe(n)`.",
    remediation: "Use expect(locator).toHaveCount(n) for web-first assertion",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-to-have-count.md",
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

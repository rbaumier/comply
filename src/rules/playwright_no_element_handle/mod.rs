//! playwright-no-element-handle — flag `page.$()` and `page.$$()`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-element-handle",
    description: "`page.$()` / `page.$$()` return ElementHandles, which are deprecated in favor of Locators.",
    remediation: "Replace `page.$('selector')` with `page.locator('selector')` \
                  and `page.$$('selector')` with \
                  `page.locator('selector').all()`. Locators auto-wait and \
                  retry, while ElementHandles are stale references that \
                  break on re-renders.",
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

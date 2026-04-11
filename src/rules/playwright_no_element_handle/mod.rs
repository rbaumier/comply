//! playwright-no-element-handle — flag `page.$()` and `page.$$()`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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

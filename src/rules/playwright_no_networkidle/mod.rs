//! playwright-no-networkidle — flag `"networkidle"` wait strategy.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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

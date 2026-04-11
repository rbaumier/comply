//! playwright-no-page-pause — flag `page.pause()` debug-only API.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-page-pause",
    description: "`page.pause()` is a debug-only API that halts test execution.",
    remediation: "Remove `page.pause()`. It opens the Playwright Inspector \
                  and blocks execution indefinitely — CI will hang until it \
                  times out.",
    severity: Severity::Error,
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

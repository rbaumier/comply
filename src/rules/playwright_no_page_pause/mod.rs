//! playwright-no-page-pause — flag `page.pause()` debug-only API.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

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
    crate::register_ts_family!(META, typescript)
}

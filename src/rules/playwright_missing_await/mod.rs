//! playwright-missing-await

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-missing-await",
    description: "Playwright async method call is missing `await`.",
    remediation: "Add `await` before the Playwright call. Without it the operation runs detached, causing flaky tests and race conditions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

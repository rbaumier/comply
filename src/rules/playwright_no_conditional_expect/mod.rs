//! playwright-no-conditional-expect — flag `expect()` inside conditionals.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-conditional-expect",
    description: "`expect()` inside `if`/`switch`/`catch` may silently skip — tests must assert unconditionally.",
    remediation: "Move the `expect()` call out of the conditional branch. \
                  A conditional assertion can silently pass when the branch \
                  is never taken, giving false confidence. Structure the \
                  test so the expected state is deterministic.",
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

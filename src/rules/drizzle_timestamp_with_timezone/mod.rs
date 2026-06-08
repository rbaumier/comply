//! drizzle-timestamp-with-timezone — bare timestamp is ambiguous.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-timestamp-with-timezone",
    description: "`timestamp('col')` is timezone-ambiguous.",
    remediation: "Add `{ withTimezone: true }` to every timestamp column. \
                  Bare timestamps are interpreted differently depending \
                  on the server's zone, silently corrupting dates.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "drizzle"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}

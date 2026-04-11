//! no-uniq-key

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-uniq-key",
    description: "Non-unique key in JSX list — `Math.random()`, `Date.now()`, or `uuid()` create new keys every render.",
    remediation: "Use a stable, unique identifier from the data (e.g., `item.id`). Random keys destroy React's reconciliation and cause performance issues.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
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

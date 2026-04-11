//! node-no-sync

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "node-no-sync",
    description: "Synchronous Node.js methods block the event loop.",
    remediation: "Use the asynchronous variant (e.g. `readFile` instead of `readFileSync`) or `fs.promises`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
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

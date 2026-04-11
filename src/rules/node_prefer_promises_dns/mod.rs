//! node-prefer-promises-dns

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "node-prefer-promises-dns",
    description: "Callback-based `dns.*` methods are discouraged.",
    remediation: "Use `dns.promises.*` or import from `dns/promises` instead of callback-based `dns` methods.",
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

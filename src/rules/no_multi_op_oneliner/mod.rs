//! no-multi-op-oneliner — reject dense chained operators on a single line.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-multi-op-oneliner",
    description: "Dense one-liners with many chained operators resist review.",
    remediation: "Extract intermediate named variables. Each step of the \
                  expression should have a name that says what it represents \
                  — `activeItems`, `prices`, `subtotal`, `total`.",
    severity: Severity::Warning,
    doc_url: None,
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

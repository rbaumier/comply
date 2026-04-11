//! reduce-initial-value

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "reduce-initial-value",
    description: "`.reduce()` without initial value throws on empty arrays.",
    remediation: "Always pass a second argument to `.reduce()`: `arr.reduce((acc, x) => acc + x, 0)`. Without it, an empty array causes `TypeError: Reduce of empty array with no initial value`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
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

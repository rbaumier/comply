//! index-of-compare-to-positive

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "index-of-compare-to-positive",
    description: "`.indexOf(…) > 0` misses index 0 — use `>= 0` or `!== -1`.",
    remediation: "Replace `> 0` with `>= 0` (or `!== -1`) to include the first element.",
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

//! prefer-dom-node-dataset

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-dataset",
    description: "Prefer `.dataset` over `.setAttribute('data-*')` / `.getAttribute('data-*')`.",
    remediation: "Replace `.setAttribute('data-foo', v)` with `.dataset.foo = v` and \
                  `.getAttribute('data-foo')` with `.dataset.foo`. The `dataset` API \
                  is cleaner and avoids string-based attribute manipulation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
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

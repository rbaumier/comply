//! require-module-attributes — flag imports/exports with empty `with {}`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "require-module-attributes",
    description: "Import/export with empty attribute list `with {}` is not allowed.",
    remediation: "Either add the required attributes (e.g. `with { type: 'json' }`) \
                  or remove the empty `with {}` clause.",
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

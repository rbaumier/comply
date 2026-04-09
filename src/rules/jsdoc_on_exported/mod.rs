//! jsdoc-on-exported — every exported function needs a JSDoc block.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-on-exported",
    description: "Exported functions must document their public contract.",
    remediation: "Add a `/** ... */` JSDoc block above the export, \
                  describing what the function does, its parameters, and \
                  what it returns. Include an @example when the call site \
                  isn't obvious.",
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

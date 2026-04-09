//! no-nullish-default-on-input — don't silently default function inputs.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-nullish-default-on-input",
    description: "Defaulting function parameters silently paves over invalid input.",
    remediation: "Don't use `??` or `||` to default a function parameter. \
                  Validate at the boundary: if the input is invalid, return \
                  a Result error. Silent defaults turn caller bugs into \
                  silent wrong answers.",
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

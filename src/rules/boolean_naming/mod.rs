//! boolean-naming — booleans must start with a predicate prefix.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "boolean-naming",
    description: "Boolean identifiers must start with is/has/should/can/will/did/was.",
    remediation: "Rename to convey the predicate: `ready` → `isReady`, \
                  `items` → `hasItems`, `retry` → `shouldRetry`. Use the \
                  positive form only — prefer `!isReady` over `isNotReady`.",
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

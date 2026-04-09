//! no-double-cast — reject `as X as Y` double casts.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-double-cast",
    description: "Double casts `as X as Y` hide misaligned types.",
    remediation: "Remove the double cast and fix the real misalignment. \
                  Either align the producer's type with the consumer's, \
                  or validate the value at the boundary using a type guard \
                  or Zod schema that actually checks the runtime shape.",
    severity: Severity::Error,
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

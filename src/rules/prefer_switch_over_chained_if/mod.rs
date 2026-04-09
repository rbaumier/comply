//! prefer-switch-over-chained-if — 4+ if/else-if on one discriminant.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-switch-over-chained-if",
    description: "Long if/else-if chains should be switch statements.",
    remediation: "Convert a 4+ branch if/else-if chain into a `switch` \
                  statement. Switch makes the discriminant obvious and \
                  lets TypeScript warn on missing cases for union types.",
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

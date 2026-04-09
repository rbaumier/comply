//! no-generic-names — reject standalone `data`/`info`/`temp`/`result`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-generic-names",
    description: "Generic names carry no meaning.",
    remediation: "Rename to describe what the value IS: `data` → \
                  `parsedOrder`, `info` → `userProfile`, `result` → \
                  `paymentReceipt`, `temp` → name the actual intermediate.",
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

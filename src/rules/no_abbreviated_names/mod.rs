//! no-abbreviated-names — reject usr/btn/cfg/ctx/msg and similar.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-abbreviated-names",
    description: "Identifier contains a banned abbreviation.",
    remediation: "Use the full word: `usr` → `user`, `cfg` → `config`, \
                  `btn` → `button`. Editors auto-complete; readers don't.",
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

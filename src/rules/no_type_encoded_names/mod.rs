//! no-type-encoded-names — reject Hungarian notation.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-type-encoded-names",
    description: "Identifiers must not encode their type (`strName`, `arrItems`).",
    remediation: "Remove the type prefix. TypeScript's type checker already \
                  tells you the type — encoding it in the name is obsolete \
                  and lies when the type changes.",
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

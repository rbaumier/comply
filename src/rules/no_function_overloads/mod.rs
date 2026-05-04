//! no-function-overloads — use unions or generics instead of overloads.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-function-overloads",
    description: "Overload signatures don't constrain the implementation.",
    remediation: "Replace overloads with a union parameter type or a \
                  generic signature. Overloads are purely ambient \
                  declarations — the compiler checks the implementation \
                  against the last signature only, which hides bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}

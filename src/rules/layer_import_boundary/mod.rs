//! layer-import-boundary

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "layer-import-boundary",
    description: "Imports that cross hexagonal architecture layers break \
                  dependency inversion and make the domain untestable.",
    remediation: "Domain must not import from infrastructure or application. \
                  Application must not import from infrastructure. \
                  Use dependency injection or ports/adapters instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["architecture"],
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

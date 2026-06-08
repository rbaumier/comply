//! layer-import-boundary

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

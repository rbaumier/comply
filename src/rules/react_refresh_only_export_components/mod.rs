//! react-refresh-only-export-components

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-refresh-only-export-components",
    description: "Non-component exports alongside component exports break React Fast Refresh (HMR).",
    remediation: "Move non-component exports (constants, utilities, types) to a separate module. Only export React components from files that also export components, so HMR can update them without a full reload.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
        ],
    }
}

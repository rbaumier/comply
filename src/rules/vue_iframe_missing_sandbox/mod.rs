//! vue-iframe-missing-sandbox

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-iframe-missing-sandbox",
    description: "`<iframe>` without a `sandbox` attribute is a security risk.",
    remediation: "Add a `sandbox` attribute to restrict the iframe capabilities.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}

//! dockerfile-label-url-format — URL-typed LABEL values must look like URLs.
//! Hadolint DL3052.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-label-url-format",
    description: "LABEL with a URL-type key must contain a valid URL.",
    remediation: "Ensure the label value starts with `http://` or `https://`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Dockerfile,
            Backend::TreeSitter(Box::new(typescript::Check)),
        )],
    }
}

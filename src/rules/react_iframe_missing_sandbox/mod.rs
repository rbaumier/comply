//! react-iframe-missing-sandbox — `<iframe>` without `sandbox` attribute.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "react-iframe-missing-sandbox",
    description: "`<iframe>` without a `sandbox` attribute is a security risk.",
    remediation: "Add a `sandbox` attribute to the `<iframe>`. The `sandbox` \
                  attribute restricts the iframe's capabilities (scripts, forms, \
                  popups) and prevents it from accessing the parent page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
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

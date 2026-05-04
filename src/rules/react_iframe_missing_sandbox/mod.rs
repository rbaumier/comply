//! react-iframe-missing-sandbox — `<iframe>` without `sandbox` attribute.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
    let backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    RuleDef {
        meta: META,
        backends,
    }
}

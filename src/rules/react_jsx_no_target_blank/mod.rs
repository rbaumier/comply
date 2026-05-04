//! react-jsx-no-target-blank — missing rel="noreferrer" with target="_blank".

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-target-blank",
    description: "`target=\"_blank\"` without `rel=\"noreferrer\"` is a security risk.",
    remediation: "Add `rel=\"noreferrer\"` (or `rel=\"noopener noreferrer\"`) when \
                  using `target=\"_blank\"`. Without it, the opened page can access \
                  `window.opener` and redirect the parent page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
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

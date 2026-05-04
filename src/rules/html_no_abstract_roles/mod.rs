//! html-no-abstract-roles
//!
//! Flags the use of abstract WAI-ARIA roles. Abstract roles exist only
//! to organize the ARIA taxonomy and must not be used on actual
//! elements.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-abstract-roles",
    description: "Abstract WAI-ARIA roles must not be used on DOM elements.",
    remediation: "Replace the abstract role with a concrete role from the WAI-ARIA specification.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],
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

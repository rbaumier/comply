//! a11y-aria-activedescendant-has-tabindex

mod oxc_typescript;
#[cfg(test)]
mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-aria-activedescendant-has-tabindex",
    description: "Elements with `aria-activedescendant` must be tabbable.",
    remediation: "Add `tabIndex={0}` (or another non-negative value) to the element that uses `aria-activedescendant`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}

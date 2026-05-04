//! a11y-role-has-required-aria-props

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
    id: "a11y-role-has-required-aria-props",
    description: "Elements with ARIA roles must have all required ARIA properties.",
    remediation: "Add the missing ARIA properties: `checkbox`/`radio` need `aria-checked`, `slider` needs `aria-valuenow`/`aria-valuemin`/`aria-valuemax`, `combobox` needs `aria-expanded`, `scrollbar` needs `aria-controls`/`aria-valuenow`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Vue, Backend::Text(Box::new(vue::Check))),
        ],
    }
}

//! a11y-html-has-lang

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
    id: "a11y-html-has-lang",
    description: "The `<html>` element must have a `lang` attribute.",
    remediation: "Add `lang=\"en\"` (or the appropriate language code) to the `<html>` element.",
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

//! a11y-anchor-ambiguous-text

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
    id: "a11y-anchor-ambiguous-text",
    description: "Flag `<a>` elements with ambiguous text like \"click here\" or \"read more\".",
    remediation: "Use descriptive link text that indicates the purpose of the link, e.g., \"View documentation\" instead of \"click here\".",
    severity: Severity::Warning,
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

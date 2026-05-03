//! Requires explicit target origin in postMessage calls.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "post-message-origin",
    description: "Requires explicit target origin in `postMessage()` calls.",
    remediation: "Specify a target origin instead of `'*'`: `postMessage(data, 'https://example.com')`.",
    severity: Severity::Error,
    doc_url: Some("https://rules.sonarsource.com/javascript/RSPEC-2819"),
    categories: &["security", "sonarjs"],
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

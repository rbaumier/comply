//! rn-raw-string-in-text — string/number children must live inside `<Text>`.
//!
//! React Native throws at runtime when text nodes appear outside a `<Text>`
//! component. Catching this at lint time avoids a red box on first render.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-raw-string-in-text",
    description: "Strings and numbers as JSX children must be wrapped in `<Text>`.",
    remediation: "Wrap the string/number child in `<Text>...</Text>`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
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

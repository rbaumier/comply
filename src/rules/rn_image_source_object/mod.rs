//! rn-image-source-object — `<Image source>` must be an object, not a string.
//!
//! React Native's `<Image>` expects `source={{ uri: '...' }}` or a `require()`
//! result — a bare string will silently render nothing.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-image-source-object",
    description: "`<Image source=\"url\">` is invalid — source must be `{ uri }` or `require()`.",
    remediation: "Use `source={{ uri: 'https://...' }}` or `source={require('./img.png')}`.",
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

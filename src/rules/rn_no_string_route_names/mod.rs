//! rn-no-string-route-names — ban string route names in `navigation.navigate(...)`.
//!
//! Expo Router provides typed paths via `router.push('/path')`; passing a bare
//! string route name bypasses that type-checking.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-no-string-route-names",
    description: "`navigation.navigate('Name', ...)` bypasses Expo Router's typed paths.",
    remediation: "Use `router.push('/typed/path')` from expo-router instead.",
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

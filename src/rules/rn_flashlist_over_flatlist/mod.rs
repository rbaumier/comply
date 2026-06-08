//! rn-flashlist-over-flatlist — prefer `FlashList` over `FlatList`.
//!
//! `@shopify/flash-list` produces dramatically better scroll performance than
//! the built-in `react-native` FlatList on most devices.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-flashlist-over-flatlist",
    description: "Importing `FlatList` from `react-native` is discouraged; use FlashList.",
    remediation: "Import `FlashList` from `@shopify/flash-list`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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

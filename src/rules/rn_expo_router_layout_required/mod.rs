//! rn-expo-router-layout-required — each Expo Router group directory needs `_layout.tsx`.
//!
//! Expo Router relies on a `_layout` file at each routable directory level to
//! compose navigation. A file importing `expo-router` inside a directory
//! without `_layout.*` almost always indicates a routing bug.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-expo-router-layout-required",
    description: "Directories that import `expo-router` must contain a `_layout` file.",
    remediation: "Add `_layout.tsx` (or `.ts` / `.jsx` / `.js`) to the directory.",
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

//! rn-no-inline-styles — forbid inline `style={{ ... }}` objects.
//!
//! RN's `StyleSheet.create` interns styles and gives them stable references.
//! Inline objects re-allocate every render and defeat memoisation.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-no-inline-styles",
    description: "Inline style objects allocate on every render and break memoisation.",
    remediation: "Move styles to `StyleSheet.create(...)` or wrap in `useMemo`.",
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

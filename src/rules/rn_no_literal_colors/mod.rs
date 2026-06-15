//! rn-no-literal-colors — forbid color literals in React Native styles.
//!
//! A hard-coded color string in a `style` prop or `StyleSheet.create(...)` call
//! cannot adapt to a theme (dark mode, accessibility). The color belongs in a
//! named constant or theme variable referenced by the style.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-no-literal-colors",
    description: "Color literals in React Native styles can't adapt to a theme.",
    remediation: "Move the color to a named constant or theme variable and reference it.",
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

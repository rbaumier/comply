//! rn-flashlist-estimated-item-size — `<FlashList>` requires `estimatedItemSize`.
//!
//! Without `estimatedItemSize`, FlashList falls back to measuring and logs a
//! runtime warning. Providing it is required for production performance.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-flashlist-estimated-item-size",
    description: "`<FlashList>` is missing the `estimatedItemSize` prop.",
    remediation: "Add `estimatedItemSize={<px>}` (approximate row height).",
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

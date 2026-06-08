//! ui-no-z-index-9999 — z-index values above 100 are almost always a sign of
//! a z-index arms race.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-z-index-9999",
    description: "`z-index` value above 100 — use a structured layering system instead.",
    remediation: "Define z-index layers as named constants (e.g. `Z_MODAL = 50`, \
                  `Z_TOOLTIP = 60`) and keep values under 100.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],

    skip_in_test_dir: true,
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

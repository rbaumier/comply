//! ui-no-permanent-will-change — flag inline `willChange` styles other than
//! `'auto'`. `will-change` should be applied dynamically right before an
//! animation and removed after; leaving it permanently wastes GPU memory.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-permanent-will-change",
    description: "Inline `willChange` is permanent — `will-change` should be applied dynamically, \
                  not baked into static styles.",
    remediation: "Apply `will-change` only during the active animation (e.g. on hover/focus) and \
                  remove it after, or set it to `'auto'`.",
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

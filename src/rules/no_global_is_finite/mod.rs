//! no-global-is-finite — prefer `Number.isFinite` over the global `isFinite`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-global-is-finite",
    description: "Use `Number.isFinite` instead of the global `isFinite`.",
    remediation: "Replace `isFinite(value)` with `Number.isFinite(value)`. The global `isFinite` \
                  coerces its argument to a number first, so `isFinite('')` is `true`; \
                  `Number.isFinite` does not coerce and is unambiguous.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["suspicious"],

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

//! zod-require-multipleof-currency — currency fields require `.multipleOf(0.01)`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-require-multipleof-currency",
    description: "Currency-bearing number schemas that accept arbitrary floats let \
                  through sub-cent precision errors (e.g. `1.2345`), which causes \
                  off-by-penny bugs downstream.",
    remediation: "Constrain to two decimals with `.multipleOf(0.01)` (or use integer \
                  minor units: `.int().nonnegative()` representing cents).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],

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

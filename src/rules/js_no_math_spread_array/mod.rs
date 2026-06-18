//! js-no-math-spread-array — `Math.min(...array)` / `Math.max(...array)`
//! risks a stack overflow on large arrays (engines cap argument counts
//! around ~65k–100k).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "js-no-math-spread-array",
    description: "`Math.min(...array)` / `Math.max(...array)` — stack-overflow risk on \
                  large arrays.",
    remediation: "Use a reduce or for-loop: \
                  `array.reduce((a, b) => a < b ? a : b, Infinity)` for min, \
                  or `-Infinity` and `>` for max.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],

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

//! no-zero-fractions — flag `1.0`, `2.00` where the fractional part is
//! all zeros. TS/JS only: in Rust, `1.0` is idiomatic and required for
//! explicit f64 typing (`1.0` vs `1` = f64 vs i32).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-zero-fractions",
    description: "Disallow number literals with zero fractions or dangling dots.",
    remediation: "Remove the unnecessary `.0` fraction — write `1` instead of `1.0`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

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

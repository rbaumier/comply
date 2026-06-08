//! perf-img-fetchpriority-high — flag LCP/hero images without fetchpriority="high",
//! and reject conflicting `fetchpriority="high"` + `loading="lazy"` combos.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-img-fetchpriority-high",
    description: "Hero/LCP images should declare `fetchpriority=\"high\"` and must not be lazy-loaded.",
    remediation: "Add `fetchpriority=\"high\"` to the LCP image, and remove `loading=\"lazy\"` on it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],

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

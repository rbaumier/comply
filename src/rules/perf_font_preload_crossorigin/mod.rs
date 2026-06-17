//! perf-font-preload-crossorigin — `<link rel="preload" as="font">` must
//! declare `crossorigin` (fonts are always fetched in CORS mode) and
//! `type="font/woff2"` so the preload can match the CSSOM request.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-font-preload-crossorigin",
    description: "`<link rel=\"preload\" as=\"font\">` must include `crossorigin` and `type=\"font/woff2\"`.",
    remediation: "Add `crossorigin` and `type=\"font/woff2\"` to the font preload link.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],

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

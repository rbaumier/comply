//! perf-no-render-blocking-css — a `<link rel="stylesheet">` without a
//! `media` attribute blocks first paint. Non-critical stylesheets should
//! declare `media="print"` (flipped to `all` via onload) or a specific
//! media query so the browser can defer them.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-no-render-blocking-css",
    description: "`<link rel=\"stylesheet\">` without a `media` attribute blocks first paint.",
    remediation: "Add a `media` attribute (e.g. `media=\"print\" onLoad=\"this.media='all'\"`) or inline critical CSS.",
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

//! react-no-interleaved-layout-rw — layout-read/style-write interleaving (layout thrash).

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-interleaved-layout-rw",
    description: "Reads of layout properties (`offsetWidth`, `getBoundingClientRect`, …) \
                  interleaved with `.style.*` writes in the same function force sync \
                  layout on every write.",
    remediation: "Batch reads first, writes second — or schedule writes inside \
                  `requestAnimationFrame` after all reads complete.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "web-performance"],

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

//! react-require-content-visibility — large `.map()` lists rendered without virtualization.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-require-content-visibility",
    description: "A `.map()` in JSX producing 20+ items with no virtualization wrapper \
                  and no `content-visibility: auto` hint paints every off-screen item.",
    remediation: "Wrap the list in a virtualizer (`react-window`, `react-virtuoso`) \
                  or set `style={{ contentVisibility: 'auto' }}` on each row.",
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

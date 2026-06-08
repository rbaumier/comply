//! xstate-no-infinite-loop

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-infinite-loop",
    description: "XState `always` transitions without a guard that stay in (or re-target) the same state cause infinite evaluation loops.",
    remediation: "Add guard to always transition or target different state",
    severity: Severity::Error,
    doc_url: Some("https://stately.ai/docs/eventless-transitions"),
    categories: &["xstate"],

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

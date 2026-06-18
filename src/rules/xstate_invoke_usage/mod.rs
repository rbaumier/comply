//! xstate-invoke-usage

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-invoke-usage",
    description: "`invoke` must be an object (or array of objects) with at least a `src` property.",
    remediation: "Add `src` to the invoke object. Optional keys: `onDone`, `onError`, `id`, `input`, `systemId`.",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/invoke"),
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

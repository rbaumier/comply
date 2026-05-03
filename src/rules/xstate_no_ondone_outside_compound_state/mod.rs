//! xstate-no-ondone-outside-compound-state

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-ondone-outside-compound-state",
    description: "XState `onDone` is only valid on compound states (with nested `states`) or invoking states (with `invoke`).",
    remediation: "onDone only valid on compound states (with states) or invoking states (with invoke)",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/final-states"),
    categories: &["xstate"],
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

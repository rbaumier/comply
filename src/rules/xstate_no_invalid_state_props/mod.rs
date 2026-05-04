mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;
use crate::rules::backend::Backend;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-invalid-state-props",
    description: "Unknown property on an XState state node — likely a typo or misplaced config.",
    remediation: "Use only valid XState state node properties",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/state-nodes"),
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

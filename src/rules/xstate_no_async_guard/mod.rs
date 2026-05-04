//! xstate-no-async-guard

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-async-guard",
    description: "XState `guard`/`cond` properties must be synchronous — async functions are not supported.",
    remediation: "Guards must be synchronous, use actors for async logic",
    severity: Severity::Error,
    doc_url: Some("https://stately.ai/docs/guards"),
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

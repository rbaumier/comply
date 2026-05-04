//! xstate-entry-exit-action

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-entry-exit-action",
    description: "`entry` and `exit` must be a string, a function, or an array of those.",
    remediation: "Use `entry: 'actionName'`, `entry: () => {}`, or `entry: ['a', 'b']`. Do not pass a plain object.",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/actions#entry-and-exit-actions"),
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

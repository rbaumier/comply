//! elysia-scope-missing

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-scope-missing",
    description: "Plugin defines lifecycle hooks but no scope — hooks won't propagate to the parent app.",
    remediation: "Add `as: 'global'` or `as: 'scoped'` to the hook (or call `.as('scoped')` on the plugin) so hooks apply to the consumer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
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

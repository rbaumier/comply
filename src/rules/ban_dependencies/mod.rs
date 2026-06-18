//! Bans imports of legacy/heavy dependencies.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ban-dependencies",
    description: "Bans imports of legacy or heavy dependencies (lodash, moment, underscore).",
    remediation: "Use native alternatives or lighter libraries (date-fns, es-toolkit).",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/nicolo-ribaudo/eslint-plugin-e18e"),
    categories: &["imports", "performance"],

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

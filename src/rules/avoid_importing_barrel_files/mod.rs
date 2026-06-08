//! avoid-importing-barrel-files

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "avoid-importing-barrel-files",
    description: "Importing from a barrel (`index`) file in the same project hurts tree-shaking and inflates startup cost.",
    remediation: "Import directly from the module that defines the symbol instead of going through the barrel.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/thepassle/eslint-plugin-barrel-files"),
    categories: &["imports"],

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

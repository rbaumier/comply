//! no-import-dist

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-import-dist",
    description: "Imports should not target `dist/` build output directories.",
    remediation: "Import from package entry point, not dist/",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],

    // Importing build output is only a smell in shippable source. Test files
    // never ship, and runners like `node:test` (no TypeScript support) must
    // import the compiled artifact from `dist/` to verify the published output.
    skip_in_test_dir: true,
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

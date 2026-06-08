//! import-enforce-node-protocol-usage — require `node:` prefix on native imports.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "import-enforce-node-protocol-usage",
    description: "Native Node modules should be imported with the `node:` protocol prefix.",
    remediation: "Replace `import fs from \"fs\"` with `import fs from \"node:fs\"`. The explicit protocol prevents accidental shadowing by an npm package of the same name and makes the import's source obvious.",
    severity: Severity::Warning,
    doc_url: Some("https://nodejs.org/api/esm.html#node-imports"),
    categories: &["import", "node"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

//! import-no-named-as-default — warn when a default import's local name
//! matches a named export of the source module. This is usually a mistake:
//! the user likely meant `import { name } from '…'`, not `import name from '…'`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "import-no-named-as-default",
    description: "Default import should not share a name with a named export of the source.",
    remediation: "Use a named import `import { name }` instead of a default import, \
                  or rename the default import to avoid confusion.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-named-as-default.md",
    ),
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

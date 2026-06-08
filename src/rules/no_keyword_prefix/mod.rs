//! no-keyword-prefix — disallow identifiers starting with `new` or `class`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-keyword-prefix",
    description: "Do not prefix identifiers with keyword `new` or `class`.",
    remediation: "Rename the identifier to remove the keyword prefix. \
                  For example, `newUser` -> `user`, `classNames` -> `names` or `cssNames`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

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

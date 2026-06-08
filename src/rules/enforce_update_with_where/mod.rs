//! enforce-update-with-where

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "enforce-update-with-where",
    description: "`db.update(table).set(...)` without `.where(...)` updates every row in the table.",
    remediation: "Add a `.where(condition)` clause to bound the update.",
    severity: Severity::Error,
    doc_url: Some(
        "https://github.com/sivaprasadreddy/eslint-plugin-drizzle#enforce-update-with-where",
    ),
    categories: &["database", "drizzle"],

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

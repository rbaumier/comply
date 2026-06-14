//! drizzle-findfirst-without-where

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-findfirst-without-where",
    description: "`.findFirst()` without a `where:` clause returns an arbitrary row — almost always a bug.",
    remediation: "Pass `{ where: ... }` to scope the query, or use `.findFirst({ orderBy: ... })` if the row choice is intentional.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "drizzle", "database"],

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

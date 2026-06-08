//! migration-needs-rollback
//!
//! AST-based detection of migration files that declare `up` but no
//! `down` / `rollback`. Walks function-like AST nodes (declarations,
//! methods, object pairs, `exports.up =` assignments) so identifiers
//! containing the substring `up` (`setup`, `lookup`, …) cannot trigger
//! the rule.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-rollback",
    description: "Migration without a `down`/rollback function is irreversible.",
    remediation: "Add an explicit `down()` / `rollback()` function to every migration. Irreversible migrations prevent quick recovery from bad deploys. Make data migrations idempotent with `ON CONFLICT DO NOTHING`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "migrations"],

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
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

//! migration-needs-rollback
//!
//! AST-based detection of migration files that declare `up` but no
//! `down` / `rollback`. Walks function-like AST nodes (declarations,
//! methods, object pairs, `exports.up =` assignments) so identifiers
//! containing the substring `up` (`setup`, `lookup`, …) cannot trigger
//! the rule.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-rollback",
    description: "Migration without a `down`/rollback function is irreversible.",
    remediation: "Add an explicit `down()` / `rollback()` function to every migration. Irreversible migrations prevent quick recovery from bad deploys. Make data migrations idempotent with `ON CONFLICT DO NOTHING`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "migrations"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}

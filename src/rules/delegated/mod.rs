//! Oxlint-delegated rules — grouped by plugin family.
//!
//! Each submodule defines rules that share the same oxlint plugin prefix.
//! Every rule in this tree uses `Backend::Oxlint { rule }` exclusively —
//! there's no per-language tree-sitter implementation, a thin wrapper
//! that (a) contributes its config-key to the runtime-generated oxlintrc
//! and (b) carries a RuleMeta so comply can remap the diagnostic rule-id
//! and remediation message when oxlint reports a violation.
//!
//! Grouping by plugin family (6 files instead of 33 folders) keeps the
//! boilerplate contained — these rules have no real implementation, so a
//! folder per rule would be pure ceremony.

mod eslint;
mod import;
mod oxc;
mod promise;
mod ts;
mod unicorn;

use crate::rules::RuleDef;

/// Collect every delegated rule into a single flat list.
pub fn register_all() -> Vec<RuleDef> {
    let mut rules = Vec::new();
    rules.extend(eslint::register_all());
    rules.extend(ts::register_all());
    rules.extend(import::register_all());
    rules.extend(unicorn::register_all());
    rules.extend(promise::register_all());
    rules.extend(oxc::register_all());
    rules
}

//! Delegated rules — rules executed by external tools (oxlint, tsgolint).
//!
//! Each submodule defines rules that share the same backend.
//! Every rule in this tree uses `Backend::Oxlint { rule }` or
//! `Backend::Tsgolint { rule }` exclusively — a thin wrapper
//! that (a) contributes its config-key to the runtime-generated config
//! and (b) carries a RuleMeta so comply can remap the diagnostic rule-id
//! and remediation message when the tool reports a violation.
//!
//! Grouping by plugin family keeps the boilerplate contained — these rules
//! have no real implementation, so a folder per rule would be pure ceremony.

mod eslint;
mod import;
mod nextjs;
mod oxc;
mod promise;
mod react;
mod ts;
pub mod tsgolint;
pub mod type_aware;
mod unicorn;
mod vue;

use crate::rules::RuleDef;

/// Collect every oxlint-delegated rule into a single flat list.
pub fn register_all() -> Vec<RuleDef> {
    let mut rules = Vec::new();
    rules.extend(eslint::register_all());
    rules.extend(ts::register_all());
    rules.extend(import::register_all());
    rules.extend(unicorn::register_all());
    rules.extend(promise::register_all());
    rules.extend(oxc::register_all());
    rules.extend(vue::register_all());
    rules.extend(nextjs::register_all());
    rules.extend(react::register_all());
    rules
}

/// Collect tsgolint-delegated rules (type-aware, only with `--type-aware`).
pub fn register_tsgolint() -> Vec<RuleDef> {
    tsgolint::register_all()
}

/// Collect comply's custom type-aware rules (sidecar-backed, only with
/// `--type-aware`).
pub fn register_type_aware() -> Vec<RuleDef> {
    type_aware::register_all()
}

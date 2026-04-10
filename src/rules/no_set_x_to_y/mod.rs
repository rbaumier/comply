//! no-set-x-to-y — flag function names like `setStatusToClosed`.
//!
//! These names encode the IMPLEMENTATION (we set a status field to a value)
//! instead of the INTENT (we close the account). They're a code smell from
//! the language-typescript skill: "intent over implementation". Renaming
//! `setStatusToClosed` → `closeAccount` makes the call site self-documenting
//! and decouples callers from the storage shape.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-set-x-to-y",
    description: "Function names like setStatusToClosed encode implementation, not intent.",
    remediation: "Rename to express the INTENT, not the storage operation: \
                  `setStatusToClosed` → `closeAccount`, `setRoleToAdmin` → `promoteToAdmin`. \
                  Callers should read like a story, not a database update.",
    severity: Severity::Error,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

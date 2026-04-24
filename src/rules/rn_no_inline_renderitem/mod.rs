//! rn-no-inline-renderitem — forbid inline arrow functions in `renderItem`.
//!
//! Inline arrows re-allocate on every render, defeating virtualised list
//! memoisation. Extract to a stable component or a `useCallback`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-no-inline-renderitem",
    description: "Inline arrow functions in `renderItem` break list virtualisation.",
    remediation: "Extract `renderItem` to a stable component or `useCallback`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

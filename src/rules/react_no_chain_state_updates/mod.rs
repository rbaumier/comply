//! react-no-chain-state-updates — flag `useEffect` callbacks that call
//! multiple `setX(...)` setters in a row.
//!
//! Chaining state updates across an effect schedules multiple render passes
//! (React 17 batching doesn't cover async callbacks, and auto-batching still
//! requires the same task). It is almost always better to:
//! - compute the derived value during render,
//! - combine the state into a single object / reducer,
//! - or use `flushSync` / `startTransition` deliberately.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-chain-state-updates",
    description: "A single `useEffect` callback triggers multiple setState calls.",
    remediation: "Combine the updates into one reducer / object, or move the derivation into render.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

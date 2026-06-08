//! react-no-chain-state-updates — flag `useEffect` callbacks that call
//! multiple `setX(...)` setters in a row.
//!
//! Chaining state updates across an effect schedules multiple render passes
//! (React 17 batching doesn't cover async callbacks, and auto-batching still
//! requires the same task). It is almost always better to:
//! - compute the derived value during render,
//! - combine the state into a single object / reducer,
//! - or use `flushSync` / `startTransition` deliberately.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-chain-state-updates",
    description: "A single `useEffect` callback triggers multiple setState calls.",
    remediation: "Combine the updates into one reducer / object, or move the derivation into render.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

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

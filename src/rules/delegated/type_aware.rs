//! Custom type-aware rules executed by comply's typescript-go sidecar.
//!
//! Unlike the `tsgolint` family, these are not typescript-eslint rules — they
//! need arbitrary type queries (`getTypeAtLocation`, structural comparison)
//! that no fixed linter exposes, so comply drives a TypeScript checker itself
//! via `crate::typeaware`. Each rule here carries only its `RuleMeta`; the
//! actual logic lives in the sidecar and is keyed by the rule id. They run
//! only when `--type-aware` is passed.

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef};

pub fn register_all() -> Vec<RuleDef> {
    vec![entry(
        "no-redundant-nullish-coalescing-null",
        "`?? null` / `?? undefined` is redundant when the left operand's type already includes that nullish value.",
        "Drop the `?? null` (or `?? undefined`) — it cannot change the value or the type.",
    )]
}

fn entry(id: &'static str, description: &'static str, remediation: &'static str) -> RuleDef {
    let backends: Vec<(Language, Backend)> = [Language::TypeScript, Language::Tsx]
        .iter()
        .map(|&lang| (lang, Backend::TypeAware))
        .collect();

    RuleDef {
        meta: RuleMeta {
            id,
            description,
            remediation,
            severity: Severity::Warning,
            doc_url: None,
            categories: &["typescript", "type-aware"],
        },
        backends,
    }
}

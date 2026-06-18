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
    vec![
        entry(
            "no-duplicate-type-definition",
            Severity::Warning,
            "Two or more named types share an identical object shape — a likely copy-paste.",
            "Consolidate the structurally identical types into a single shared type.",
        ),
        entry(
            "no-redundant-nullish-coalescing-null",
            Severity::Warning,
            "`?? null` / `?? undefined` is redundant when the left operand's type already includes that nullish value.",
            "Drop the `?? null` (or `?? undefined`) — it cannot change the value or the type.",
        ),
        entry(
            "ts-no-in-operator",
            Severity::Warning,
            "The `in` operator probes an object's shape by hand instead of validating it.",
            "Parse external input with a schema (e.g. Zod) to obtain a typed value, or discriminate an owned union with a `kind` tag + exhaustive `switch`. Reserve `in` for a caught error or a non-serializable runtime object (DOM dataset, Playwright Page/Locator).",
        ),
        entry(
            "ts-no-typeof-operator",
            Severity::Warning,
            "The `typeof` operator stands in for validating a boundary value or discriminating an owned union.",
            "Parse external `unknown` with a schema (e.g. Zod), and discriminate an owned union with a `kind` tag + exhaustive `switch`. Reserve `typeof` for an environment guard (`typeof window`), a caught error, a `z.preprocess` normaliser, or discriminating a union whose variant is non-serializable (function/JSX).",
        ),
    ]
}

fn entry(
    id: &'static str,
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    let backends: Vec<(Language, Backend)> = [Language::TypeScript, Language::Tsx]
        .iter()
        .map(|&lang| (lang, Backend::TypeAware))
        .collect();

    RuleDef {
        meta: RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
            categories: &["typescript", "type-aware"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        backends,
    }
}

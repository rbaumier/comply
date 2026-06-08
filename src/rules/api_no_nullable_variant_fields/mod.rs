//! api-no-nullable-variant-fields — flag interfaces that lean on many
//! optional fields sharing a prefix/suffix (e.g. `cancelReason?`,
//! `cancelledAt?`, `cancelledBy?`). This pattern encodes a state machine
//! in optional flags, which forces clients to guess invariants instead
//! of relying on a discriminated union.
//!
//! Optional members typed as `never` (`page?: never`) are skipped —
//! that is the mutually-exclusive-props / phantom-key pattern, where
//! the key MUST be absent, the opposite of a state flag.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-nullable-variant-fields",
    description: "Interfaces must not encode state via clusters of optional fields; use discriminated unions.",
    remediation: "Replace the optional cluster with a `status: 'cancelled'; cancelReason: string; cancelledAt: string` variant in a discriminated union.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],

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

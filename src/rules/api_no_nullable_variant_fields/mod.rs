//! api-no-nullable-variant-fields — flag interfaces that lean on many
//! optional fields sharing a prefix/suffix (e.g. `cancelReason?`,
//! `cancelledAt?`, `cancelledBy?`). This pattern encodes a state machine
//! in optional flags, which forces clients to guess invariants instead
//! of relying on a discriminated union.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-nullable-variant-fields",
    description: "Interfaces must not encode state via clusters of optional fields; use discriminated unions.",
    remediation: "Replace the optional cluster with a `status: 'cancelled'; cancelReason: string; cancelledAt: string` variant in a discriminated union.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

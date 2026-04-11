//! no-weak-keys

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-weak-keys",
    description: "Weak cryptographic key lengths are vulnerable to brute-force attacks.",
    remediation: "Use RSA >= 2048 bits and EC >= P-256. Prefer Ed25519 or P-384 for new keys.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}

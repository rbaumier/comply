//! no-insecure-jwt

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-insecure-jwt",
    description: "Weak JWT algorithms (`none`, `HS256`) allow token forgery or trivial brute-force.",
    remediation: "Use asymmetric algorithms (`RS256`, `ES256`) for JWT verification. Never allow `algorithm: 'none'` and avoid `HS256` unless you control both issuer and verifier.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

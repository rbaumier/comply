//! axum-jwt-secret-hardcoded

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-jwt-secret-hardcoded",
    description: "JWT signing secret passed to `jsonwebtoken` `EncodingKey::from_secret`/`DecodingKey::from_secret` is a hardcoded literal — leaks via source control.",
    remediation: "Read the secret from `std::env::var(\"JWT_SECRET\")` or a secret manager and pass \
                  the resolved bytes (e.g. `EncodingKey::from_secret(secret.as_bytes())`); never \
                  embed the secret as a string/byte-string literal.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

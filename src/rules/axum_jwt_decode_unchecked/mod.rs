//! axum-jwt-decode-unchecked

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-jwt-decode-unchecked",
    description: "A `jsonwebtoken::decode`/`decode_header` result is consumed with `.unwrap()`/`.expect()` — an invalid or expired token panics instead of being rejected.",
    remediation: "Handle the `Result` instead of unwrapping it: propagate it with `?` or `match` on \
                  the `Err` arm and return `401`. `.unwrap()`/`.expect()` turns a forged, malformed, \
                  or expired token into a panic (a request-triggered denial-of-service) rather than a \
                  rejected request.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

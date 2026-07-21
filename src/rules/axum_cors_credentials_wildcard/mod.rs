//! axum-cors-credentials-wildcard

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-cors-credentials-wildcard",
    description: "Combining CORS credentials with a wildcard origin is rejected by browsers and exposes the axum API to every site.",
    remediation: "Pair `.allow_credentials(true)` with a specific origin: \
                  `.allow_origin(\"https://your-domain.com\".parse::<HeaderValue>().unwrap())`. \
                  `.allow_origin(Any)` and `CorsLayer::very_permissive()` cannot be combined with credentials safely.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

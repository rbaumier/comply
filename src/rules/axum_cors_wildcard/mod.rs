//! axum-cors-wildcard

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-cors-wildcard",
    description: "Permissive CORS allows any origin to access the axum API.",
    remediation: "Restrict the origin: \
                  `CorsLayer::new().allow_origin(\"https://your-domain.com\".parse::<HeaderValue>().unwrap())`. \
                  `CorsLayer::permissive()`, `CorsLayer::very_permissive()`, and \
                  `.allow_origin(Any)` let every origin reach the API.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

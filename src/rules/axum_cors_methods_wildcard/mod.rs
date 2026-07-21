//! axum-cors-methods-wildcard

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-cors-methods-wildcard",
    description: "Combining CORS credentials with wildcard methods lets every HTTP verb reach the axum API from credentialed requests.",
    remediation: "Pair `.allow_credentials(true)` with an explicit method list: \
                  `.allow_methods([Method::GET, Method::POST])`. \
                  `.allow_methods(Any)` and `CorsLayer::very_permissive()` cannot be combined with credentials safely.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

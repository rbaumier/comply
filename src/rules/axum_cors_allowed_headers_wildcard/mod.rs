//! axum-cors-allowed-headers-wildcard

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-cors-allowed-headers-wildcard",
    description: "Combining CORS credentials with wildcard allowed headers weakens the preflight contract: browsers reject a credentialed request whose allowed headers answer with `*`.",
    remediation: "Pair `.allow_credentials(true)` with an explicit header list: \
                  `.allow_headers([AUTHORIZATION, CONTENT_TYPE])`. \
                  `.allow_headers(Any)` and `CorsLayer::very_permissive()` cannot be combined with credentials safely.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

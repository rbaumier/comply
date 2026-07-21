//! axum-jwt-cookie-no-httponly

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-jwt-cookie-no-httponly",
    description: "Cookie carrying a JWT (`jsonwebtoken::encode`) is built without `http_only` — the token is readable from JavaScript (XSS).",
    remediation: "Add `.http_only(true)` to the `Cookie::build(...)` chain that stores a \
                  JWT so the token cannot be read by scripts. Setting `.http_only(false)` \
                  leaves it exposed.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

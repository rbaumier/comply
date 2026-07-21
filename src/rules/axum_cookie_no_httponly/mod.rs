//! axum-cookie-no-httponly

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-cookie-no-httponly",
    description: "Cookie built without `http_only` — it is readable from JavaScript (XSS vector).",
    remediation: "Add `.http_only(true)` to the `Cookie::build(...)` chain so the \
                  cookie is not exposed to JavaScript. Setting `.http_only(false)` \
                  leaves it readable from scripts.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

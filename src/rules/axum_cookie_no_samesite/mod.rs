//! axum-cookie-no-samesite

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-cookie-no-samesite",
    description: "Cookie built without `same_site` — it inherits inconsistent cross-browser SameSite defaults.",
    remediation: "Add `.same_site(SameSite::Lax)` (or `SameSite::Strict` for \
                  sensitive cookies) to the `Cookie::build(...)` chain so the \
                  SameSite policy is explicit rather than browser-dependent.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

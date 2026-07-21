//! axum-cookie-no-secure

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-cookie-no-secure",
    description: "Cookie built without `secure` — it can be sent over plain HTTP.",
    remediation: "Add `.secure(true)` to the `Cookie::build(...)` chain so the \
                  cookie is only sent over HTTPS. Setting `.secure(false)` leaves \
                  it exposed on plain HTTP.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

//! axum-bearer-not-validated

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-bearer-not-validated",
    description: "A Bearer token is extracted via `TypedHeader<Authorization<Bearer>>` but the \
                  extracted credential is never read — the handler accepts any token.",
    remediation: "Read the extracted credential (`auth.token()`) and validate it — compare it \
                  against your token store or verify the JWT — then return `401` when it is \
                  invalid. A handler that extracts the bearer header but never touches the token \
                  accepts every request, forged tokens included.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

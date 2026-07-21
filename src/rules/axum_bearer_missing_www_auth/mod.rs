//! axum-bearer-missing-www-auth

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-bearer-missing-www-auth",
    description: "A `401 Unauthorized` response returned as \
                  `(StatusCode::UNAUTHORIZED, body).into_response()` in a file that never sets a \
                  `WWW-Authenticate` header — an RFC 7235 / RFC 6750 violation.",
    remediation: "Attach a `WWW-Authenticate` challenge to the 401 response, e.g. return \
                  `(StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, \"Bearer\")], body)` or \
                  set the header on a `Response::builder()`. RFC 7235 requires every 401 to name \
                  the accepted authentication scheme so clients know how to authenticate.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

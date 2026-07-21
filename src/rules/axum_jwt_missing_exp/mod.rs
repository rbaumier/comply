//! axum-jwt-missing-exp

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-jwt-missing-exp",
    description: "`jsonwebtoken` `Validation` has `validate_exp` set to `false` — expired tokens are accepted.",
    remediation: "Leave `validate_exp` at its default (`true`) so `jsonwebtoken` rejects expired \
                  tokens; a `Validation` built with `Validation::new(alg)` already validates `exp`. \
                  Remove the `validate_exp = false` assignment.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

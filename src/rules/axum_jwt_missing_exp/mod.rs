//! axum-jwt-missing-exp

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-jwt-missing-exp",
    description: "`jsonwebtoken` `Validation` has `validate_exp` or `validate_aud` set to \
                  `false` — expired tokens, or tokens issued for another service, are accepted.",
    remediation: "Leave `validate_exp` and `validate_aud` at their default (`true`) so \
                  `jsonwebtoken` rejects expired tokens and tokens for another audience; a \
                  `Validation` built with `Validation::new(alg)` already validates both. Remove \
                  the `= false` assignment and set the expected audience with `set_audience`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}

//! security-require-pkce-oauth

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-require-pkce-oauth",
    description: "OAuth authorize URLs must include a `code_challenge` (PKCE).",
    remediation: "Generate a PKCE verifier/challenge and append `code_challenge` and `code_challenge_method` to the authorize URL.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

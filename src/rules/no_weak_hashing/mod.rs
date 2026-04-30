//! no-weak-hashing

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-weak-hashing",
    description: "MD5 and SHA-1 are cryptographically broken — use SHA-256 or stronger.",
    remediation: "Replace `createHash('md5')` / `createHash('sha1')` with `createHash('sha256')` or use `crypto.subtle.digest('SHA-256', …)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}

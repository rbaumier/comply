//! no-deprecated-cipher

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-deprecated-cipher",
    description: "`createCipher()` derives the key from a password using MD5 — use `createCipheriv()` instead.",
    remediation: "Replace `crypto.createCipher(algo, password)` with `crypto.createCipheriv(algo, key, iv)`. The deprecated function uses MD5 to derive the key, which is insecure and non-standard.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}

//! no-weak-hashing
//!
//! The harm is a broken hash reaching production security code (password
//! hashing, signatures, content-addressed integrity boundaries). Test files
//! (`skip_in_test_dir`) never ship such a primitive — a weak hash there is a
//! non-cryptographic fixture checksum (e.g. asserting an encoder round-trip
//! produces an expected MD5 digest), so it is out of scope and not flagged. A
//! production use of the same weak hash is still flagged in its own (non-test)
//! file.

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-weak-hashing",
    description: "MD5 and SHA-1 are cryptographically broken — use SHA-256 or stronger.",
    remediation: "Replace `createHash('md5')` / `createHash('sha1')` with `createHash('sha256')` or use `crypto.subtle.digest('SHA-256', …)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

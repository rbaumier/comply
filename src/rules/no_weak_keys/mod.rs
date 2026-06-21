//! no-weak-keys
//!
//! The harm is a weak crypto key reaching production. Test files
//! (`skip_in_test_dir`) never ship a real credential — a weak key generated
//! there is deliberate negative-test input (e.g. asserting the library rejects
//! a 1024-bit RSA key), so it is out of scope and not flagged. A production copy
//! of the same weak key is still flagged in its own (non-test) file.

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-weak-keys",
    description: "Weak cryptographic key lengths are vulnerable to brute-force attacks.",
    remediation: "Use RSA >= 2048 bits and EC >= P-256. Prefer Ed25519 or P-384 for new keys.",
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

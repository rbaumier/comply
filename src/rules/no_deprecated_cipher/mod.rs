//! no-deprecated-cipher

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-deprecated-cipher",
    description: "`createCipher()` derives the key from a password using MD5 — use `createCipheriv()` instead.",
    remediation: "Replace `crypto.createCipher(algo, password)` with `crypto.createCipheriv(algo, key, iv)`. The deprecated function uses MD5 to derive the key, which is insecure and non-standard.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}

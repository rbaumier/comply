//! no-weak-hashing

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-weak-hashing",
    description: "MD5 and SHA-1 are cryptographically broken — use SHA-256 or stronger.",
    remediation: "Replace `createHash('md5')` / `createHash('sha1')` with `createHash('sha256')` or use `crypto.subtle.digest('SHA-256', …)`.",
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

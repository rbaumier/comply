//! no-unverified-certificate

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-unverified-certificate",
    description: "Disabling SSL certificate verification enables man-in-the-middle attacks.",
    remediation: "Remove `rejectUnauthorized: false` and `NODE_TLS_REJECT_UNAUTHORIZED = '0'`. Use proper CA certificates instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    let mut backends: Vec<(Language, Backend)> = TS_FAMILY
        .iter()
        .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
        .collect();
    backends.push((Language::Rust, Backend::TreeSitter(Box::new(rust::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}

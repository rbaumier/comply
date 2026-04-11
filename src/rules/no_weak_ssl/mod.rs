//! no-weak-ssl

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-weak-ssl",
    description: "Weak SSL/TLS protocol versions are insecure.",
    remediation: "Use TLSv1.2 or TLSv1.3. Older protocols (SSLv2, SSLv3, TLSv1.0, TLSv1.1) have known vulnerabilities.",
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

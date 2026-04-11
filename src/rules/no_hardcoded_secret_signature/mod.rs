//! no-hardcoded-secret-signature

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-hardcoded-secret-signature",
    description: "Hardcoded secrets in JWT signing or crypto operations leak credentials into source control.",
    remediation: "Load signing keys from environment variables or a secrets manager (e.g., `process.env.JWT_SECRET`).",
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

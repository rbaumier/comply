//! no-unverified-hostname

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-unverified-hostname",
    description: "Disabling TLS hostname verification allows man-in-the-middle attacks.",
    remediation: "Remove the `checkServerIdentity` override. Setting it to a no-op function or `null` disables hostname verification, making TLS connections vulnerable to MITM attacks.",
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

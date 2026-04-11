//! no-ecb-mode

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-ecb-mode",
    description: "ECB cipher mode is insecure — identical plaintext blocks produce identical ciphertext.",
    remediation: "Use CBC, CTR, or GCM mode instead of ECB. ECB does not provide semantic security because it encrypts identical blocks to the same ciphertext, leaking patterns.",
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

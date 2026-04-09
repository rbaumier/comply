//! no-auth-token-in-localstorage — XSS exfiltration risk.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-auth-token-in-localstorage",
    description: "Auth tokens in localStorage are XSS-exfiltratable.",
    remediation: "Store auth tokens in httpOnly cookies. The browser \
                  attaches them automatically and JavaScript cannot read \
                  them, so a successful XSS can't steal the session.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}

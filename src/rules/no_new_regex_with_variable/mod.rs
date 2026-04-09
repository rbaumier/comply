//! no-new-regex-with-variable — ReDoS risk.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-new-regex-with-variable",
    description: "`new RegExp(variable)` enables ReDoS attacks.",
    remediation: "Replace dynamic regex construction with a literal regex \
                  or a vetted safe-regex library. User-controlled patterns \
                  can trigger exponential backtracking and freeze the event loop.",
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

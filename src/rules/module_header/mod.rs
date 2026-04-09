//! module-header — every file starts with a JSDoc describing its purpose.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "module-header",
    description: "Every file must start with a JSDoc module-header comment.",
    remediation: "Add a `/** */` block at the top of the file with two \
                  things: (1) What this module does, (2) How it works. \
                  A reader opening the file should know its purpose before \
                  scrolling to the first declaration.",
    severity: Severity::Warning,
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

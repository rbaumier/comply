//! empty-catch-block — flag `catch` blocks with an empty body.
//!
//! An empty catch silently swallows errors. Either rethrow, log + recover,
//! or return a Result error. The "I'll deal with it later" branch is the
//! one that breaks production at 2am with no signal.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "empty-catch-block",
    description: "Empty catch swallows errors silently.",
    remediation: "An empty catch block hides failures. Either rethrow with context, \
                  log and recover, or convert to a Result error. If the error truly \
                  doesn't matter, add a comment explaining why.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
        ],
    }
}

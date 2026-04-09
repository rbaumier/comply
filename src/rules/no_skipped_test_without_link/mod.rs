//! no-skipped-test-without-link — track every `.skip` to a ticket.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-skipped-test-without-link",
    description: "Every `.skip` must reference a tracked issue.",
    remediation: "Add a comment above the `.skip` with an issue reference \
                  (`#123`, `ABC-456`, or a URL) so the skip can be revived \
                  later. Untracked skips become permanent coverage holes.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

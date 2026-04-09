//! banned-identifiers — rename any identifier starting with `process` /
//! `handle` / `data` / `do` / `execute` / `run` / `perform` on a word
//! boundary. These verbs describe mechanics, not intent.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "banned-identifiers",
    description: "Banned prefixes describe mechanics, not intent.",
    remediation: "Rename to express what this accomplishes, not how. \
                  `processOrder` → `fulfillOrder`, `handlePayment` → `chargeCustomer`.",
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
            // Rust: clippy::disallowed_names (configurable; doesn't do
            // word-boundary matching out of the box). See rust.rs.
            (Language::Rust, Backend::Clippy { lint: "clippy::disallowed_names" }),
        ],
    }
}

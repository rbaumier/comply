//! no-generic-names — reject standalone `data`/`info`/`temp`/`result`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-generic-names",
    description: "Generic names carry no meaning.",
    remediation: "Rename to describe what the value IS: `data` → \
                  `parsedOrder`, `info` → `userProfile`, `result` → \
                  `paymentReceipt`, `temp` → name the actual intermediate.",
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
            // Rust: clippy::disallowed_names with custom list in clippy.toml.
            (Language::Rust, Backend::Clippy { lint: "clippy::disallowed_names" }),
        ],
    }
}

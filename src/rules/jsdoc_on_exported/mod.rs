//! jsdoc-on-exported — every exported function needs a JSDoc block.

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-on-exported",
    description: "Exported functions must document their public contract.",
    remediation: "Add a `/** ... */` JSDoc block above the export, \
                  describing what the function does, its parameters, and \
                  what it returns. Include an @example when the call site \
                  isn't obvious.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, typescript).backends;
    backends.push((Language::Rust, Backend::Clippy { lint: "missing_docs" }));
    backends.push((Language::Vue, Backend::Text(Box::new(text::Check))));
    RuleDef { meta: META, backends }
}

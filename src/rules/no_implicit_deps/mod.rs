//! no-implicit-deps

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-implicit-deps",
    description: "Import of a bare specifier that is not a known Node.js builtin — may be an unlisted dependency.",
    remediation: "Ensure the package is listed in `package.json` dependencies. Bare specifier imports that are neither relative paths nor Node.js builtins may break when not explicitly installed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
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

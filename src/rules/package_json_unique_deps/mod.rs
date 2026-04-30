//! package-json-unique-deps

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "package-json-unique-deps",
    description: "A package in both dependencies and devDependencies is ambiguous — \
                  npm/pnpm silently picks one, which surprises consumers.",
    remediation: "Keep each package in exactly one section. Production deps go in \
                  `dependencies`; build-only tools go in `devDependencies`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["package-json"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::JavaScript, Backend::Text(Box::new(text::Check)))],
    }
}

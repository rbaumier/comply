//! toml-tables-order — require top-level TOML tables to appear in
//! alphabetical order so config files are predictable to scan.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "toml-tables-order",
    description: "Top-level TOML tables should be declared in alphabetical order.",
    remediation: "Reorder your `[table]` / `[[array_of_tables]]` headers so that \
                  consecutive tables are in alphabetical order. Sibling sub-tables \
                  under a common prefix are compared as a whole dotted key.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["toml"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Toml, Backend::Text(Box::new(text::Check)))],
    }
}

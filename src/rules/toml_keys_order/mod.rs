//! toml-keys-order — require keys within each TOML table to be declared in
//! alphabetical order so diffs and merges are predictable.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "toml-keys-order",
    description: "Keys inside a TOML table should be declared in alphabetical order.",
    remediation: "Reorder the keys inside each `[table]` so that consecutive \
                  `key = value` entries are in alphabetical order. Applies per \
                  table — the order resets after every `[section]` header.",
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

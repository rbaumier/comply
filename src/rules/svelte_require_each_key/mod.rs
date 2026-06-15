//! svelte-require-each-key

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "svelte-require-each-key",
    description: "Keyed `{#each}` blocks let Svelte track list items across updates; without a key Svelte updates by position, moving state between items when the list changes.",
    remediation: "Add a `(key)` clause to the `{#each}` block, e.g. `{#each items as item (item.id)}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["svelte"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Svelte, Backend::Text(Box::new(text::Check)))],
    }
}

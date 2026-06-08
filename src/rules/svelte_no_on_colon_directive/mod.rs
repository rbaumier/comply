//! svelte-no-on-colon-directive

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "svelte-no-on-colon-directive",
    description: "Svelte 4 `on:event` directives are deprecated — use the Svelte 5 `onevent` attribute form.",
    remediation: "Replace `on:click={handler}` with `onclick={handler}`. Same for `on:submit`, `on:input`, etc.",
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

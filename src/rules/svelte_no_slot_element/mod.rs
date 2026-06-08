//! svelte-no-slot-element

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "svelte-no-slot-element",
    description: "`<slot>` is the Svelte 4 child-rendering primitive — Svelte 5 prefers snippets and `{@render}`.",
    remediation: "Replace `<slot />` and `<slot name=\"x\" />` with snippet props rendered via `{@render children?.()}` / `{@render header?.()}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["svelte"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Html, Backend::Text(Box::new(text::Check)))],
    }
}

//! svelte-no-legacy-reactive

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "svelte-no-legacy-reactive",
    description: "Legacy `$:` reactive declarations are deprecated — use `$derived` or `$effect` runes (Svelte 5).",
    remediation: "Replace `$: x = expr;` with `let x = $derived(expr);`. For side-effects, use `$effect(() => { ... })`.",
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

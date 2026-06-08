//! svelte-no-effect-for-derived

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "svelte-no-effect-for-derived",
    description: "`$effect` should not be used to compute a value from other reactive state — use `$derived` instead.",
    remediation: "Replace `$effect(() => { x = expr; })` with `let x = $derived(expr);`. `$derived` is purpose-built for computed values; `$effect` is for side-effects (DOM, network, logging).",
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

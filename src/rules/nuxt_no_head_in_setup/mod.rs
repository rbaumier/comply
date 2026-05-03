//! nuxt-no-head-in-setup

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-head-in-setup",
    description: "Setting `head` via component options bypasses the typed `useHead()` composable.",
    remediation: "Call `useHead({ title, meta, link })` from `<script setup>` or inside `setup()`.",
    severity: Severity::Warning,
    doc_url: Some("https://nuxt.com/docs/getting-started/seo-meta"),
    categories: &["nuxt", "seo"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

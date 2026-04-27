//! nuxt-no-head-in-setup

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
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
    crate::register_ts_family!(META, typescript)
}

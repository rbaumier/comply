mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-sfc-section-order",
    description: "SFC sections must be ordered: `<script setup>` → `<template>` → `<style>`.",
    remediation: "Reorder sections: script first, template second, style last.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],

    // Test-fixture SFCs intentionally use non-canonical section order (e.g.
    // Vue 2-style template-before-script) to exercise parsers and runtime
    // behavior — they are inputs under test, not shipped components.
    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}

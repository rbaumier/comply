//! vue-no-v-if-with-v-for — forbid `v-if` and `v-for` on the same element.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-v-if-with-v-for",
    description: "`v-if` and `v-for` on the same element have ambiguous priority and are a Vue anti-pattern.",
    remediation: "Move the `v-if` to a wrapper `<template>` around the `v-for`, or filter the list in a computed.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}

//! use-vue-valid-template-root — enforce valid Vue `<template>` root usage.
//!
//! Reports only the first root-level `<template>` element of a Vue SFC. When
//! that `<template>` has a `src` attribute it must be empty; otherwise it must
//! contain non-whitespace content. A whitespace-only template counts as empty.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-vue-valid-template-root",
    description: "The root `<template>` must contain content, unless it carries a `src` attribute in which case it must be empty.",
    remediation: "Add content inside the root `<template>`, or remove its content when using a `src` attribute.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}

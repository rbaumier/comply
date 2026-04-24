//! vue-scoped-styles-preferred — flag `<style>` without `scoped` in SFCs.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-scoped-styles-preferred",
    description: "`<style>` without `scoped` leaks selectors globally.",
    remediation: "Add `scoped` to the `<style>` tag unless the styles are intentionally global.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}

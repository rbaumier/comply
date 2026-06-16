//! use-vue-hyphenated-attributes — enforce kebab-case attribute names in Vue
//! templates.
//!
//! Vue's style guide recommends hyphenated (kebab-case) attribute and prop names
//! in templates so they stay consistent and distinct from camelCase/PascalCase
//! JavaScript identifiers. This rule flags plain HTML attributes, `:foo`
//! shorthand bindings, and `v-bind:`/`v-model:` directive arguments whose name
//! is not kebab-case or pure-lowercase. Attributes on SVG-exclusive elements
//! (e.g. `<svg viewBox>`) are skipped.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-vue-hyphenated-attributes",
    description: "Template attribute and prop names should be hyphenated (kebab-case).",
    remediation: "Rename the attribute to kebab-case, e.g. `:some-prop` instead of `:someProp`.",
    severity: Severity::Warning,
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

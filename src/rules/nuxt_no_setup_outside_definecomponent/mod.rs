//! nuxt-no-setup-outside-definecomponent

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-setup-outside-definecomponent",
    description: "`<script setup>` composables called outside `defineComponent` in options-API files leak across instances.",
    remediation: "Either move to `<script setup>` or wrap the logic inside `defineComponent({ setup() {} })`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["nuxt", "vue"],
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

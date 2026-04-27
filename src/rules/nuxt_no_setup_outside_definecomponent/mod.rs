//! nuxt-no-setup-outside-definecomponent

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
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
    crate::register_ts_family!(META, typescript)
}

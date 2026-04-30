mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-script-setup-required",
    description: "`<script>` without `setup` attribute uses Options-API-style Composition API — use `<script setup>` instead.",
    remediation: "Change `<script lang=\"ts\">` to `<script setup lang=\"ts\">` and remove the `setup()` function.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}

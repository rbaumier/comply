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
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],

    // Test-fixture SFCs intentionally use Options-API `<script>` with a
    // `setup()` function to exercise that authoring style — requiring
    // `<script setup>` would defeat the purpose of the fixture.
    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}

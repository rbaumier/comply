mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-pinia-store-to-refs",
    description: "Destructuring a Pinia store without `storeToRefs()` loses reactivity.",
    remediation: "Use `const { count } = storeToRefs(useCounterStore())` to preserve reactivity.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}

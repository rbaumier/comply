//! vue-no-ref-as-operand — Vue 3 ref used without `.value`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-ref-as-operand",
    description: "Using a Vue 3 `ref` directly in an arithmetic / comparison expression compares the wrapper object, not the underlying value.",
    remediation: "Unwrap with `.value`: `count.value + 1`, not `count + 1`. The ref is an object — JS coerces it to `[object Object]` in string contexts and to NaN in numeric ones.",
    severity: Severity::Error,
    doc_url: Some("https://eslint.vuejs.org/rules/no-ref-as-operand.html"),
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}

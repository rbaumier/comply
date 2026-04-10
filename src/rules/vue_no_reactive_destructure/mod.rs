//! vue-no-reactive-destructure — destructuring reactive() breaks reactivity.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-reactive-destructure",
    description: "Destructuring `reactive()` breaks reactivity — use `toRefs()` or `ref()`.",
    remediation: "`const { count } = reactive({ count: 0 })` copies the primitive — \
                  `count` is now a plain number, not reactive. Use \
                  `const { count } = toRefs(state)` to get a ref that stays connected, \
                  or use `ref()` directly for each field.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}

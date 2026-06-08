//! vue-inject-key-typed — require `InjectionKey<T>` symbols, not string keys.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-inject-key-typed",
    description: "String keys in `provide()`/`inject()` lose type information.",
    remediation: "Declare `const MY_KEY: InjectionKey<T> = Symbol('my-key')` and pass it to provide/inject.",
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

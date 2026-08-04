//! no-common-grab-bag — reject a `common` / `utils` / `helpers` / `shared` /
//! `misc` / `util` module that has accreted a large exported surface.
//!
//! Configure the bar with `[rules.no-common-grab-bag] min_exports` in
//! `comply.toml`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-common-grab-bag",
    description: "A `common`/`utils`/`helpers`/`shared`/`misc`/`util` module \
                  holding a large exported surface names none of what it owns.",
    remediation: "Split the module along the concerns it accumulated and name \
                  each part after what that part owns.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    let backends: Vec<_> = [
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Rust,
        Language::Vue,
    ]
    .into_iter()
    .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
    .collect();
    RuleDef {
        meta: META,
        backends,
    }
}

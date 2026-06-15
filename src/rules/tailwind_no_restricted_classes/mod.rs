//! tailwind-no-restricted-classes — flag classnames matching a
//! user-configured blocklist.
//!
//! The rule is opt-in via `comply.toml`:
//!
//! ```toml
//! [rules.tailwind-no-restricted-classes]
//! classes = ["bg-white", "text-black", "space-y-px"]
//! ```
//!
//! When the `classes` list is absent or empty, the check is a no-op — no
//! opinionated default blocklist fires on projects that never adopted one.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-restricted-classes",
    description: "User-configured blocklist of Tailwind classes — typically used to ban legacy spacing tokens, ad-hoc colors, or deprecated utility names.",
    remediation: "Use the project-approved equivalent. If the class is needed for a one-off, escape via the project's design-token override mechanism.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

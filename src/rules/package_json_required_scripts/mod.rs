//! package-json-required-scripts — flag a `package.json` that is missing one
//! or more user-configured required scripts.
//!
//! The rule is opt-in via `comply.toml`:
//!
//! ```toml
//! [rules.package-json-required-scripts]
//! scripts = ["build", "test", "lint"]
//! ```
//!
//! When the `scripts` list is absent or empty, the check is a no-op — comply
//! has no opinionated default set of required scripts.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "package-json-required-scripts",
    description: "A `package.json` is missing a script the project requires — consistent \
                  scripts across packages let each one be run reliably from the repo root.",
    remediation: "Add the missing script(s) to the `scripts` section of `package.json`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["package-json"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Json, Backend::Text(Box::new(text::Check)))],
    }
}

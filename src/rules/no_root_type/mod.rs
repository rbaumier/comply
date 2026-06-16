//! no-root-type
//!
//! Configuration:
//!
//! ```toml
//! [rules.no-root-type]
//! disallow = ["mutation", "subscription"]
//! ```

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-root-type",
    description: "A GraphQL object type whose name is on the project's disallowed list (e.g. `Mutation` or `Subscription`) violates a deliberate schema-design constraint.",
    remediation: "Use a different root type, or remove the name from `[rules.no-root-type] disallow` if this root type should be allowed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["graphql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::GraphQl, Backend::Text(Box::new(text::Check)))],
    }
}

//! node-no-top-level-await

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-top-level-await",
    description: "Top-level `await` is forbidden in published modules.",
    remediation: "Wrap the `await` expression inside an `async` function.",
    severity: Severity::Error,
    doc_url: Some(
        "https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-top-level-await.md",
    ),
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

//! no-unsupported-node-builtins — flag use of Node.js APIs that aren't
//! available in the minimum Node version declared in `engines.node`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unsupported-node-builtins",
    description: "Node.js API not available in the minimum version declared in `engines.node`.",
    remediation: "Either bump the minimum Node.js version in `engines.node`, or use a polyfill.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-unsupported-features/node-builtins.md",
    ),
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

//! prefer-node-protocol — use `node:` prefix for Node.js builtins.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-node-protocol",
    description: "Prefer `node:` protocol for Node.js builtin imports.",
    remediation: "Replace bare builtin specifiers (`fs`, `path`, …) with \
                  `node:fs`, `node:path`. The `node:` prefix makes it \
                  unambiguous that the import targets a Node.js builtin, \
                  not a user-land package with the same name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

//! prefer-node-protocol — use `node:` prefix for Node.js builtins.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}

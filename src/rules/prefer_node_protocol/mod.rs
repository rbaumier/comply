//! prefer-node-protocol — use `node:` prefix for Node.js builtins.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

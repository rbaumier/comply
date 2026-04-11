//! prefer-module — prefer ESM over CommonJS.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-module",
    description: "Prefer ESM (`import`/`export`) over CommonJS (`require`/`module.exports`).",
    remediation: "Replace `require()` with `import`, `module.exports` / \
                  `exports.x` with `export`, and `__dirname` / `__filename` \
                  with `import.meta.dirname` / `import.meta.filename`.",
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

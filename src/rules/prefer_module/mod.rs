//! prefer-module — prefer ESM over CommonJS.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;
use crate::rules::backend::Backend;

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
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

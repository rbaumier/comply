//! prefer-import-meta-properties — prefer `import.meta.filename` / `import.meta.dirname`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-import-meta-properties",
    description: "Prefer `import.meta.filename` and `import.meta.dirname` over legacy techniques.",
    remediation: "Replace `fileURLToPath(import.meta.url)` with `import.meta.filename` \
                  and `dirname(fileURLToPath(import.meta.url))` with `import.meta.dirname`. \
                  Node.js 21.2+ and Bun support these properties natively.",
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

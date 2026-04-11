//! prefer-import-meta-properties — prefer `import.meta.filename` / `import.meta.dirname`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-import-meta-properties",
    description: "Prefer `import.meta.filename` and `import.meta.dirname` over legacy techniques.",
    remediation: "Replace `fileURLToPath(import.meta.url)` with `import.meta.filename` \
                  and `dirname(fileURLToPath(import.meta.url))` with `import.meta.dirname`. \
                  Node.js 21.2+ and Bun support these properties natively.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

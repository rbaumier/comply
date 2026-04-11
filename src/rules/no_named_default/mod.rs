//! no-named-default — disallow named usage of default import.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-named-default",
    description: "Disallow `import { default as foo }` — use `import foo` instead.",
    remediation: "Replace `import { default as foo } from './m'` with \
                  `import foo from './m'`. The named form is verbose and \
                  obscures the intent of importing the default export.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

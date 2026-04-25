//! zod-require-error-messages — `.refine()` should carry an error message.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-require-error-messages",
    description: "`.refine()` without an error message produces unhelpful validation errors.",
    remediation: "Add `{ message: 'descriptive error' }` as the second argument to `.refine()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

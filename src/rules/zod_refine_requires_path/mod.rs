//! zod-refine-requires-path — object-level `.refine()` must attach its
//! error to a specific field via `path: [...]`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-refine-requires-path",
    description: "`z.object().refine()` without `path:` attaches the error to the whole object, not a specific field.",
    remediation: "Add `path: ['fieldName']` to the refine options so form errors appear on the correct field.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

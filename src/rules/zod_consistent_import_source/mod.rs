//! zod-consistent-import-source — flag imports from non-standard zod subpaths.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-consistent-import-source",
    description: "Imports from non-standard zod subpaths (e.g., `zod/v4`, `zod/mini`) cause \
                  inconsistent schemas and mixed API surfaces across the codebase.",
    remediation: "Use consistent import source for zod",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}

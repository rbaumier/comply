//! module-header — every file starts with a JSDoc describing its purpose.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "module-header",
    description: "Every file must start with a JSDoc module-header comment.",
    remediation: "Add a `/** */` block at the top of the file with two \
                  things: (1) What this module does, (2) How it works. \
                  A reader opening the file should know its purpose before \
                  scrolling to the first declaration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_clippy_marker!(META, typescript, "missing_docs")
}
